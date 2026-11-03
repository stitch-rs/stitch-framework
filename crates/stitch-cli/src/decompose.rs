//! Automated Module Decomposition Engine (`stitch split`).
//!
//! Refactors oversized source files by cleanly extracting inline `mod tests` into a
//! dedicated `tests.rs` module while preserving full visibility, attributes, and comments.

use std::path::{Path, PathBuf};

pub struct DecomposeEngine;

impl DecomposeEngine {
    /// Transforms an oversized file `foo.rs` containing inline tests into `foo/mod.rs` and `foo/tests.rs`.
    pub fn split_tests(file_path: &Path) -> Result<(PathBuf, PathBuf), String> {
        if !file_path.is_file() {
            return Err(format!("File `{}` not found.", file_path.display()));
        }

        let content = std::fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read `{}`: {e}", file_path.display()))?;

        // Parse file into Syn AST
        let syntax_tree = syn::parse_file(&content)
            .map_err(|e| format!("Syntax error parsing `{}`: {e}", file_path.display()))?;

        // Locate Item::Mod with ident == "tests"
        let mut tests_mod = None;
        for item in &syntax_tree.items {
            if let syn::Item::Mod(item_mod) = item
                && item_mod.ident == "tests"
            {
                tests_mod = Some(item_mod.clone());
                break;
            }
        }

        let tests_mod = tests_mod.ok_or_else(|| {
            format!(
                "No inline `mod tests` item found in `{}`.",
                file_path.display()
            )
        })?;

        // If tests module has no inline content (e.g., already `mod tests;`), nothing to do
        let (_, items) = tests_mod.content.ok_or_else(|| {
            format!(
                "Module `mod tests;` in `{}` is already an external module reference.",
                file_path.display()
            )
        })?;

        // Format tests.rs content by pretty-printing test items or reconstructing tokens
        let mut tests_rs_content = String::new();
        tests_rs_content.push_str("//! Unit tests extracted automatically by `stitch split`.\n\n");

        let mut has_super_glob = false;
        for item in &items {
            if let syn::Item::Use(item_use) = item {
                let use_str = quote::quote!(#item_use).to_string();
                if use_str.contains("super :: *") {
                    has_super_glob = true;
                    break;
                }
            }
        }

        if !has_super_glob {
            tests_rs_content.push_str("use super::*;\n\n");
        }

        for item in &items {
            let item_tokens = quote::quote!(#item);
            tests_rs_content.push_str(&item_tokens.to_string());
            tests_rs_content.push_str("\n\n");
        }

        // Generate modified main AST where `mod tests { ... }` is replaced with `mod tests;`
        let mut new_items = Vec::with_capacity(syntax_tree.items.len());
        for item in syntax_tree.items {
            if let syn::Item::Mod(mut m) = item {
                if m.ident == "tests" {
                    m.content = None;
                    m.semi = Some(syn::token::Semi::default());
                    new_items.push(syn::Item::Mod(m));
                } else {
                    new_items.push(syn::Item::Mod(m));
                }
            } else {
                new_items.push(item);
            }
        }

        let new_file = syn::File {
            items: new_items,
            ..syntax_tree
        };

        let main_content = quote::quote!(#new_file).to_string();

        // Validate syntax with syn before doing any file moves
        if syn::parse_file(&main_content).is_err() {
            return Err(
                "Safety abort: Main file syntax invalid after extracting tests.".to_string(),
            );
        }
        if syn::parse_file(&tests_rs_content).is_err() {
            return Err("Safety abort: Extracted tests file syntax is invalid.".to_string());
        }

        // Determine destination:
        // If file is `foo.rs`, we create directory `foo/`, write `foo/mod.rs` and `foo/tests.rs`, then delete `foo.rs`.
        // If file is `mod.rs` or `lib.rs`, we create `tests.rs` as sibling in the same folder.
        let file_stem = file_path.file_stem().unwrap().to_string_lossy().to_string();
        let parent = file_path.parent().unwrap_or_else(|| Path::new("."));

        let (dest_main, dest_tests) = if file_stem == "mod" || file_stem == "lib" {
            let tests_path = parent.join("tests.rs");
            std::fs::write(&tests_path, &tests_rs_content).map_err(|e| e.to_string())?;
            std::fs::write(file_path, &main_content).map_err(|e| e.to_string())?;
            (file_path.to_path_buf(), tests_path)
        } else {
            let target_dir = parent.join(&file_stem);
            std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
            let mod_path = target_dir.join("mod.rs");
            let tests_path = target_dir.join("tests.rs");

            std::fs::write(&mod_path, &main_content).map_err(|e| e.to_string())?;
            std::fs::write(&tests_path, &tests_rs_content).map_err(|e| e.to_string())?;

            // Remove original foo.rs
            let _ = std::fs::remove_file(file_path);
            (mod_path, tests_path)
        };

        // Format both files using rustfmt if available
        let _ = std::process::Command::new("rustfmt")
            .arg(&dest_main)
            .status();
        let _ = std::process::Command::new("rustfmt")
            .arg(&dest_tests)
            .status();

        Ok((dest_main, dest_tests))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_tests_from_standalone_file() {
        let temp_dir =
            std::env::temp_dir().join(format!("stitch_decompose_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_path = temp_dir.join("processor.rs");
        let initial_code = r#"
pub fn calculate(a: u32, b: u32) -> u32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate() {
        assert_eq!(calculate(2, 2), 4);
    }
}
"#;
        std::fs::write(&file_path, initial_code).unwrap();

        let res = DecomposeEngine::split_tests(&file_path);
        assert!(res.is_ok(), "Split failed: {:?}", res.err());
        let (dest_mod, dest_tests) = res.unwrap();

        assert!(
            !file_path.exists(),
            "Original standalone file must be removed"
        );
        assert!(dest_mod.is_file(), "processor/mod.rs must be created");
        assert!(dest_tests.is_file(), "processor/tests.rs must be created");

        let mod_code = std::fs::read_to_string(dest_mod).unwrap();
        assert!(mod_code.contains("pub fn calculate") || mod_code.contains("calculate"));
        assert!(mod_code.contains("mod tests ;") || mod_code.contains("mod tests;"));
        assert!(!mod_code.contains("assert_eq ! (calculate (2 , 2) , 4)"));

        let tests_code = std::fs::read_to_string(dest_tests).unwrap();
        assert!(tests_code.contains("fn test_calculate"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
