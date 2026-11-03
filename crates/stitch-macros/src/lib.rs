//! Procedural macro attribute suite and compile-time architecture linter for SMA.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Item, ItemStruct, parse_macro_input};

mod newtype_pipeline;
mod pipeline;
mod rules;
mod scrooge;
mod taxonomy;
mod visitor;

use newtype_pipeline::*;
use pipeline::*;
use rules::*;
use scrooge::*;
use taxonomy::*;

#[proc_macro_attribute]
pub fn layer(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    match &item {
        Item::Struct(s) => {
            if let Err(e) = verify_suffix(&s.ident, "Layer", SMA_TAXO_001) {
                return e.to_compile_error().into();
            }
            if let Err(e) = inspect_struct_fields(&s.fields) {
                return e.to_compile_error().into();
            }
            quote! { #item }.into()
        }
        Item::Impl(item_impl) => {
            if let Err(e) = verify_layer_impl(item_impl) {
                return e.to_compile_error().into();
            }
            quote! { #item }.into()
        }
        _ => quote! { #item }.into(),
    }
}

#[proc_macro_attribute]
pub fn terminal(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    match &item {
        Item::Struct(s) => {
            if let Err(e) = verify_suffix(&s.ident, "Terminal", SMA_TAXO_002) {
                return e.to_compile_error().into();
            }
            if let Err(e) = inspect_struct_fields(&s.fields) {
                return e.to_compile_error().into();
            }
            quote! { #item }.into()
        }
        Item::Impl(item_impl) => {
            if let Err(e) = verify_terminal_impl(item_impl) {
                return e.to_compile_error().into();
            }
            quote! { #item }.into()
        }
        _ => quote! { #item }.into(),
    }
}

#[proc_macro_attribute]
pub fn port(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    if let Item::Trait(t) = &item {
        match verify_suffix(&t.ident, "Port", SMA_TAXO_003) {
            Ok(()) => quote! { #item }.into(),
            Err(e) => e.to_compile_error().into(),
        }
    } else {
        quote! { #item }.into()
    }
}

#[proc_macro_attribute]
pub fn adapter(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    if let Item::Struct(s) = &item {
        match verify_suffix(&s.ident, "Adapter", SMA_TAXO_004) {
            Ok(()) => quote! { #item }.into(),
            Err(e) => e.to_compile_error().into(),
        }
    } else {
        quote! { #item }.into()
    }
}

#[proc_macro_attribute]
pub fn provider(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    if let Item::Struct(s) = &item {
        #[allow(clippy::collapsible_if)]
        if let Err(e) = verify_suffix(&s.ident, "Provider", SMA_TAXO_005) {
            return e.to_compile_error().into();
        }
    }
    quote! { #item }.into()
}

#[proc_macro_attribute]
pub fn token(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ItemStruct);
    if let Err(e) = verify_suffix(&item.ident, "Token", SMA_TAXO_006) {
        return e.to_compile_error().into();
    }
    if let Err(e) = verify_transparent(&item.attrs, item.ident.span()) {
        return e.to_compile_error().into();
    }
    let assertion = synthesize_register_size_assertion(&item.ident);
    quote! {
        #item
        #assertion
    }
    .into()
}

#[proc_macro_attribute]
pub fn id(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ItemStruct);
    if let Err(e) = verify_suffix(&item.ident, "Id", SMA_TAXO_007) {
        return e.to_compile_error().into();
    }
    if let Err(e) = verify_transparent(&item.attrs, item.ident.span()) {
        return e.to_compile_error().into();
    }
    let assertion = synthesize_register_size_assertion(&item.ident);
    quote! {
        #item
        #assertion
    }
    .into()
}

#[proc_macro_attribute]
pub fn blackboard(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ItemStruct);
    if let Err(e) = verify_blackboard_suffix(&item.ident) {
        return e.to_compile_error().into();
    }
    if let Err(e) = verify_cache_alignment(&item.attrs, item.ident.span()) {
        return e.to_compile_error().into();
    }
    if let Err(e) = inspect_struct_fields(&item.fields) {
        return e.to_compile_error().into();
    }
    quote! { #item }.into()
}

#[proc_macro_attribute]
pub fn entity(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    quote! { #item }.into()
}

#[proc_macro_attribute]
pub fn value_object(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    if let Item::Struct(s) = &item {
        match inspect_struct_fields(&s.fields) {
            Ok(()) => quote! { #item }.into(),
            Err(e) => e.to_compile_error().into(),
        }
    } else {
        quote! { #item }.into()
    }
}

#[proc_macro_attribute]
pub fn event(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    if let Item::Struct(s) = &item {
        match inspect_struct_fields(&s.fields) {
            Ok(()) => quote! { #item }.into(),
            Err(e) => e.to_compile_error().into(),
        }
    } else {
        quote! { #item }.into()
    }
}

#[proc_macro_attribute]
pub fn data(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    quote! { #item }.into()
}

#[proc_macro_attribute]
pub fn query(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    match &item {
        Item::Struct(s) => {
            if let Err(e) = verify_suffix(&s.ident, "Query", SMA_TAXO_001) {
                return e.to_compile_error().into();
            }
            if let Err(e) = inspect_struct_fields(&s.fields) {
                return e.to_compile_error().into();
            }
            quote! { #item }.into()
        }
        Item::Impl(item_impl) => {
            if let Err(e) = verify_query_impl(item_impl) {
                return e.to_compile_error().into();
            }
            quote! { #item }.into()
        }
        _ => quote! { #item }.into(),
    }
}

#[proc_macro_attribute]
pub fn command(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    match &item {
        Item::Struct(s) => {
            if let Err(e) = verify_suffix(&s.ident, "Command", SMA_TAXO_001) {
                return e.to_compile_error().into();
            }
            if let Err(e) = inspect_struct_fields(&s.fields) {
                return e.to_compile_error().into();
            }
            quote! { #item }.into()
        }
        Item::Impl(item_impl) => {
            if let Err(e) = verify_command_impl(item_impl) {
                return e.to_compile_error().into();
            }
            quote! { #item }.into()
        }
        _ => quote! { #item }.into(),
    }
}

/// Generates a complete SMA U-cycle execution pipeline, Layer trait, and Terminal executor.
#[proc_macro]
pub fn define_pipeline(input: TokenStream) -> TokenStream {
    let def = parse_macro_input!(input as PipelineDef);
    expand_define_pipeline(def).into()
}

/// Declares a compile-time monomorphic SMA execution pipeline expanding into a `#[repr(transparent)]` Tuple Struct.
///
/// # Arguments
/// - `context`: Type of the root blackboard context.
/// - `intent`: Type of the intent entering the pipeline.
/// - `outcome`: Return type of successful traversal (default: `()`).
/// - `error`: Error type returned on traversal failure (default: `()`).
/// - `layers`: Ordered array of middleware layers `[LayerA, LayerB, ...]` in natural forward execution order.
/// - `terminal`: The terminal leaf handler node type.
/// - `crate`: (Optional) Root path to `stitch_core`.
///
/// # Example
/// ```rust,ignore
/// #[pipeline(
///     context = MyContext,
///     intent = MyIntent,
///     outcome = u32,
///     error = MyError,
///     layers = [AuthLayer, RateLimitLayer],
///     terminal = ExecTerminal,
/// )]
/// pub struct MyPipeline;
/// ```
#[proc_macro_attribute]
pub fn pipeline(args: TokenStream, input: TokenStream) -> TokenStream {
    let parsed_args = parse_macro_input!(args as PipelineAttrArgs);
    let item_struct = parse_macro_input!(input as ItemStruct);
    match expand_newtype_pipeline(parsed_args, item_struct, false) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Declares a wire transport framing and resync pipeline expanding into a `#[repr(transparent)]` Tuple Struct.
///
/// Defaults `context` to `WireBlackboard`, `intent` to `&[u8]`, `outcome` to `usize`, and `error` to `WireError`.
///
/// # Example
/// ```rust,ignore
/// #[wire_pipeline(
///     layers = [SwarResyncLayer, FramingLayer, CastagnoliCrcLayer, MtuChunkingLayer],
///     terminal = BufferTerminal<'b>,
/// )]
/// pub struct GameWirePipeline<'a, 'b>;
/// ```
#[proc_macro_attribute]
pub fn wire_pipeline(args: TokenStream, input: TokenStream) -> TokenStream {
    let parsed_args = parse_macro_input!(args as PipelineAttrArgs);
    let item_struct = parse_macro_input!(input as ItemStruct);
    match expand_newtype_pipeline(parsed_args, item_struct, true) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
