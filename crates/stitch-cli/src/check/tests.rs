//! Unit tests extracted automatically by `stitch split`.

use super::*;

#[test]
fn test_scan_file_syntax_error_emits_parse_diagnostic() {
    let temp_dir =
        std::env::temp_dir().join(format!("stitch_test_syntax_err_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let bad_file = temp_dir.join("broken.rs");
    std::fs::write(&bad_file, "pub struct Bad { invalid syntax here !!!").unwrap();
    let cfg = StitchConfig::default();
    let runner = CheckRunner::new(&cfg, &temp_dir);
    let diags = runner.scan_file(&bad_file);
    let _ = std::fs::remove_dir_all(&temp_dir);
    assert!(
        !diags.is_empty(),
        "Corrupted file must produce diagnostics, not empty vec!"
    );
    assert!(
        diags.iter().any(|d| d.code == "SMA-PARSE-001"),
        "Expected SMA-PARSE-001"
    );
}

#[test]
fn test_scan_file_missing_emits_io_diagnostic() {
    let non_existent = Path::new("non_existent_never_file_12345.rs");
    let cfg = StitchConfig::default();
    let runner = CheckRunner::new(&cfg, Path::new("."));
    let diags = runner.scan_file(non_existent);
    assert!(!diags.is_empty());
    assert!(
        diags.iter().any(|d| d.code == "SMA-IO-001"),
        "Expected SMA-IO-001"
    );
}

#[test]
fn test_recursive_heap_ban_detects_nested_types() {
    let code = r#"
            #[stitch::layer]
            pub struct NestedHeapLayer {
                pub opt: Option<String>,
                pub arr: [String; 4],
                pub tup: (u8, Vec<u8>),
                pub dyn_ref: &'static dyn std::fmt::Display,
            }
        "#;
    let syntax_tree = syn::parse_file(code).unwrap();
    let cfg = StitchConfig::default();
    let mut visitor = AstScanner {
        config: &cfg,
        file_path: Path::new("test_nested.rs"),
        diagnostics: Vec::new(),
        in_hot_path_fn: false,
        in_pipeline_impl: false,
        in_test: false,
    };
    visitor.visit_file(&syntax_tree);
    let scrooge_010: Vec<_> = visitor
        .diagnostics
        .iter()
        .filter(|d| d.code == "SMA-SCROOGE-010")
        .collect();
    let scrooge_011: Vec<_> = visitor
        .diagnostics
        .iter()
        .filter(|d| d.code == "SMA-SCROOGE-011")
        .collect();
    assert_eq!(
        scrooge_010.len(),
        3,
        "Expected Option<String>, [String; 4], (u8, Vec<u8>) to be caught"
    );
    assert_eq!(
        scrooge_011.len(),
        1,
        "Expected dyn std::fmt::Display to be caught"
    );
}

#[test]
fn test_hotpath_alloc_and_methods_detected() {
    let code = r#"
            pub struct TestLayer;
            impl TestLayer {
                pub fn on_enter(&self, ctx: &mut (), intent: ()) {
                    let _ = Box::new(42);
                    let _ = "hello".to_string();
                    println!("alloc logging");
                    todo!();
                }
            }
        "#;
    let syntax_tree = syn::parse_file(code).unwrap();
    let cfg = StitchConfig::default();
    let mut visitor = AstScanner {
        config: &cfg,
        file_path: Path::new("test_hotpath.rs"),
        diagnostics: Vec::new(),
        in_hot_path_fn: false,
        in_pipeline_impl: false,
        in_test: false,
    };
    visitor.visit_file(&syntax_tree);
    assert!(
        visitor
            .diagnostics
            .iter()
            .any(|d| d.code == "SMA-HOTPATH-020"),
        "Expected SMA-HOTPATH-020 for println/todo"
    );
    assert!(
        visitor
            .diagnostics
            .iter()
            .any(|d| d.code == "SMA-HOTPATH-021"),
        "Expected SMA-HOTPATH-021 for to_string"
    );
    assert!(
        visitor
            .diagnostics
            .iter()
            .any(|d| d.code == "SMA-HOTPATH-022"),
        "Expected SMA-HOTPATH-022 for Box::new"
    );
}

#[test]
fn test_stringly_typed_macro_attribute_detected() {
    let code = r#"
            #[stitch(codec = "varint", column = "dict")]
            pub struct Frame {
                pub id: u64,
            }

            #[stitch(codec = Varint, column = ColumnCodec::Dict)]
            pub struct GoodFrame {
                pub id: u64,
            }
        "#;
    let syntax_tree = syn::parse_file(code).unwrap();
    let cfg = StitchConfig::default();
    let mut visitor = AstScanner {
        config: &cfg,
        file_path: Path::new("test_stringly.rs"),
        diagnostics: Vec::new(),
        in_hot_path_fn: false,
        in_pipeline_impl: false,
        in_test: false,
    };
    visitor.visit_file(&syntax_tree);
    let stringly_diags: Vec<_> = visitor
        .diagnostics
        .iter()
        .filter(|d| d.code == "SMA-SCROOGE-016")
        .collect();
    assert_eq!(
        stringly_diags.len(),
        2,
        "Expected exactly 2 stringly-typed attribute violations in Frame"
    );
    assert!(stringly_diags[0].message.contains("\"varint\""));
    assert!(stringly_diags[1].message.contains("\"dict\""));
}
