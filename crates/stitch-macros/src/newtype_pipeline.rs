use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Error, Ident, ItemStruct, Path, Result, Token, Type, bracketed};

/// Parsed arguments for `#[pipeline(...)]` and `#[wire_pipeline(...)]`.
pub struct PipelineAttrArgs {
    pub context: Option<Type>,
    pub intent: Option<Type>,
    pub outcome: Option<Type>,
    pub error: Option<Type>,
    pub layers: Vec<Type>,
    pub terminal: Option<Type>,
    pub crate_path: Option<Path>,
    pub is_wire: bool,
}

impl Parse for PipelineAttrArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut context = None;
        let mut intent = None;
        let mut outcome = None;
        let mut error = None;
        let mut layers = Vec::new();
        let mut terminal = None;
        let mut crate_path = None;
        let mut is_wire = false;

        while !input.is_empty() {
            let key = input.call(Ident::parse_any)?;

            // Support either `:` or `=` (e.g., `layers = [...]` or `layers: [...]`)
            if input.peek(Token![=]) {
                input.parse::<Token![=]>()?;
            } else if input.peek(Token![:]) {
                input.parse::<Token![:]>()?;
            } else {
                // Standalone flag like `wire`
                let key_str = key.to_string();
                if key_str == "wire" {
                    is_wire = true;
                    if input.peek(Token![,]) {
                        input.parse::<Token![,]>()?;
                    }
                    continue;
                } else {
                    return Err(Error::new(key.span(), "expected `=` or `:` after key"));
                }
            }

            match key.to_string().as_str() {
                "context" | "ctx" => {
                    context = Some(input.parse::<Type>()?);
                }
                "intent" => {
                    intent = Some(input.parse::<Type>()?);
                }
                "outcome" => {
                    outcome = Some(input.parse::<Type>()?);
                }
                "error" | "err" => {
                    error = Some(input.parse::<Type>()?);
                }
                "layers" => {
                    let content;
                    bracketed!(content in input);
                    let punctuated: Punctuated<Type, Token![,]> =
                        Punctuated::parse_terminated(&content)?;
                    layers = punctuated.into_iter().collect();
                }
                "terminal" | "term" => {
                    terminal = Some(input.parse::<Type>()?);
                }
                "crate" => {
                    crate_path = Some(input.parse::<Path>()?);
                }
                "wire" => {
                    let lit: syn::LitBool = input.parse()?;
                    is_wire = lit.value;
                }
                other => {
                    return Err(Error::new(
                        key.span(),
                        format!(
                            "unknown pipeline attribute key `{other}`, expected `context`, `intent`, `outcome`, `error`, `layers`, `terminal`, `crate`, or `wire`"
                        ),
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(PipelineAttrArgs {
            context,
            intent,
            outcome,
            error,
            layers,
            terminal,
            crate_path,
            is_wire,
        })
    }
}

/// Expands the `#[pipeline(...)]` attribute macro on a struct definition.
pub fn expand_newtype_pipeline(
    args: PipelineAttrArgs,
    item_struct: ItemStruct,
    wire_default: bool,
) -> Result<TokenStream2> {
    let ItemStruct {
        attrs,
        vis,
        ident,
        generics,
        ..
    } = item_struct;

    let is_wire = args.is_wire || wire_default;

    let crate_path = args.crate_path.unwrap_or_else(|| {
        if is_wire {
            syn::parse_quote!(::stitch_wire)
        } else {
            syn::parse_quote!(::stitch_core)
        }
    });

    let context_ty = match args.context {
        Some(ty) => ty,
        None if is_wire => syn::parse_quote!(#crate_path::WireBlackboard),
        None => {
            return Err(Error::new(
                ident.span(),
                "missing `context` in #[pipeline(...)]; specify `context = ...` or use #[wire_pipeline(...)]",
            ));
        }
    };

    let first_lifetime = generics.lifetimes().next().map(|lt| &lt.lifetime);

    let intent_ty = match args.intent {
        Some(ty) => ty,
        None if is_wire => {
            if let Some(lt) = first_lifetime {
                syn::parse_quote!(&#lt [u8])
            } else {
                syn::parse_quote!(&'a [u8])
            }
        }
        None => {
            return Err(Error::new(
                ident.span(),
                "missing `intent` in #[pipeline(...)]; specify `intent = ...`",
            ));
        }
    };

    let outcome_ty = args.outcome.unwrap_or_else(|| {
        if is_wire {
            syn::parse_quote!(usize)
        } else {
            syn::parse_quote!(())
        }
    });

    let error_ty = args.error.unwrap_or_else(|| {
        if is_wire {
            syn::parse_quote!(#crate_path::WireError)
        } else {
            syn::parse_quote!(())
        }
    });

    let terminal_ty = args.terminal.ok_or_else(|| {
        Error::new(
            ident.span(),
            "missing `terminal` in #[pipeline(...)]; specify `terminal = TerminalType`",
        )
    })?;

    // SMA Core path for pipeline primitives (Pipeline, StackNode, TerminalNode)
    let core_path: Path = if is_wire {
        syn::parse_quote!(#crate_path::core)
    } else {
        syn::parse_quote!(#crate_path)
    };

    // 1. Build the Monomorphic Chain Type from layers + terminal.
    // Natural order: `layers = [L0, L1, L2]` -> traversal runs L0, then L1, then L2, then Terminal.
    // In StackNode representation: StackNode<L0, StackNode<L1, StackNode<L2, TerminalNode<Term>>>>.
    let mut chain_ty = quote! { #core_path::pipeline::TerminalNode<#terminal_ty> };
    for layer in args.layers.iter().rev() {
        chain_ty = quote! { #core_path::pipeline::StackNode<#layer, #chain_ty> };
    }

    // 2. Build the inner Pipeline type
    let inner_pipeline_ty = quote! {
        #core_path::pipeline::Pipeline<
            #context_ty,
            #intent_ty,
            #outcome_ty,
            #error_ty,
            #chain_ty,
        >
    };

    // 3. Build constructor argument identifiers: `layer_0, layer_1, ...`
    let layer_idents: Vec<Ident> = (0..args.layers.len())
        .map(|i| quote::format_ident!("layer_{}", i))
        .collect();

    // In constructor: start at Terminal, wrap layers in reverse order: `.wrap(L2).wrap(L1).wrap(L0)`.
    let mut build_expr = quote! {
        #core_path::pipeline::Pipeline::on_terminal(terminal)
    };
    for ident_arg in layer_idents.iter().rev() {
        build_expr = quote! { #build_expr.wrap(#ident_arg) };
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let layer_types = &args.layers;
    let chain_ident = quote::format_ident!("{}Chain", ident);

    Ok(quote! {
        #(#attrs)*
        #[repr(transparent)]
        #vis struct #ident #generics(pub #inner_pipeline_ty);

        /// The compile-time monomorphic chain type for [`#ident`].
        #vis type #chain_ident #generics = #chain_ty;

        impl #impl_generics #ident #ty_generics #where_clause {
            /// Constructs a new pipeline instance by wiring layers to the terminal handler.
            #[inline(always)]
            #vis fn new(
                #(#layer_idents: #layer_types,)*
                terminal: #terminal_ty,
            ) -> Self {
                let pipe = #build_expr;
                Self(pipe)
            }

            /// Wraps a pre-assembled monomorphic pipeline into this newtype.
            #[inline(always)]
            #vis const fn from_pipeline(pipeline: #inner_pipeline_ty) -> Self {
                Self(pipeline)
            }

            /// Dispatches an intent through the complete monomorphic U-cycle traversal.
            #[inline(always)]
            #vis fn dispatch(
                &mut self,
                ctx: &mut #context_ty,
                intent: #intent_ty,
            ) -> ::core::result::Result<#outcome_ty, #error_ty> {
                self.0.dispatch(ctx, intent)
            }

            /// Alias for [`Self::dispatch`]: executes the complete monomorphic U-cycle traversal.
            #[inline(always)]
            #vis fn run_cycle(
                &mut self,
                ctx: &mut #context_ty,
                intent: #intent_ty,
            ) -> ::core::result::Result<#outcome_ty, #error_ty> {
                self.0.dispatch(ctx, intent)
            }

            /// Borrows the underlying monomorphic [`Pipeline`].
            #[inline(always)]
            #vis fn inner(&self) -> &#inner_pipeline_ty {
                &self.0
            }

            /// Mutably borrows the underlying monomorphic [`Pipeline`].
            #[inline(always)]
            #vis fn inner_mut(&mut self) -> &mut #inner_pipeline_ty {
                &mut self.0
            }

            /// Unwraps this newtype into the underlying monomorphic [`Pipeline`].
            #[inline(always)]
            #vis fn into_inner(self) -> #inner_pipeline_ty {
                self.0
            }
        }

        impl #impl_generics ::core::ops::Deref for #ident #ty_generics #where_clause {
            type Target = #inner_pipeline_ty;

            #[inline(always)]
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl #impl_generics ::core::ops::DerefMut for #ident #ty_generics #where_clause {
            #[inline(always)]
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }
    })
}
