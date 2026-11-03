//! Automated Structural Topology Scanner & Validator for SMA Repositories.
//!
//! Enforces non-hardcoded, graph-theoretic file-tree invariants across 3 canonical
//! architectural patterns:
//! - `Facade`: Atomic library root with `src/` and no nested crates.
//! - `Uniform`: Virtual workspace with a single grouping container (e.g. `crates/*`).
//! - `Segmented`: Hexagonal multi-domain virtual workspace (e.g. `backends/*`, `hosts/*`, `services/*`).

use crate::check::Diagnostic;
use crate::config::{RuleSeverity, StitchConfig};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The recognized structural topology strategy of an SMA repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TopologyKind {
    /// Atomic library or SDK root with `src/` and no child sub-manifests.
    Facade,
    /// Homogeneous workspace with a single package container (e.g. `crates/<pkg>`).
    Uniform,
    /// Hexagonal multi-domain workspace with symmetric functional categories (e.g. `<domain>/<pkg>`).
    Segmented,
    /// Structural failure or ambiguous hierarchy.
    Malformed,
}

impl std::fmt::Display for TopologyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Facade => write!(f, "Facade (Atomic Root)"),
            Self::Uniform => write!(f, "Uniform Workspace (Single Container)"),
            Self::Segmented => write!(f, "Segmented Domain Engine (Multi-Domain Hexagonal)"),
            Self::Malformed => write!(f, "Malformed Topology"),
        }
    }
}

/// Metadata discovered for a single package manifest in the workspace tree.
#[derive(Debug, Clone)]
pub struct DiscoveredPackage {
    pub manifest_path: PathBuf,
    pub relative_path: PathBuf,
    pub package_name: Option<String>,
    pub depth: usize,
    pub parent_category: Option<String>,
}

/// Evaluation result of the repository topology.
#[derive(Debug, Clone)]
pub struct TopologyReport {
    pub detected_kind: TopologyKind,
    pub is_virtual_workspace: bool,
    pub has_root_package: bool,
    pub has_root_src: bool,
    pub discovered_packages: Vec<DiscoveredPackage>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct TopologyScanner<'a> {
    config: &'a StitchConfig,
    root_dir: &'a Path,
}

impl<'a> TopologyScanner<'a> {
    pub fn new(config: &'a StitchConfig, root_dir: &'a Path) -> Self {
        Self { config, root_dir }
    }

    /// Evaluates structural tree invariants and checks for topological violations.
    pub fn evaluate(&self) -> TopologyReport {
        let root_manifest_path = self.root_dir.join("Cargo.toml");
        let root_src_path = self.root_dir.join("src");
        let has_root_src = root_src_path.is_dir();

        let (has_root_package, is_virtual_workspace) = if root_manifest_path.is_file() {
            let content = std::fs::read_to_string(&root_manifest_path).unwrap_or_default();
            let has_pkg = content.contains("[package]");
            let has_ws = content.contains("[workspace]");
            (has_pkg, has_ws && !has_pkg)
        } else {
            (false, false)
        };

        let mut discovered = Vec::new();
        let mut depth_set = BTreeSet::new();
        let mut categories = BTreeMap::new();
        let mut diags = Vec::new();

        // Collect all non-root Cargo.toml manifests across the tree
        self.collect_manifests(
            self.root_dir,
            &PathBuf::new(),
            0,
            &mut discovered,
            &mut depth_set,
            &mut categories,
        );

        // Classify TopologyKind purely based on tree invariants without directory name hardcoding
        let detected_kind = if has_root_package && discovered.is_empty() && has_root_src {
            TopologyKind::Facade
        } else if is_virtual_workspace && !has_root_src {
            if categories.len() == 1 && depth_set.len() == 1 && depth_set.contains(&2) {
                TopologyKind::Uniform
            } else if categories.len() > 1 && depth_set.len() == 1 && depth_set.contains(&2) {
                TopologyKind::Segmented
            } else {
                TopologyKind::Malformed
            }
        } else {
            TopologyKind::Malformed
        };

        // --- Invariant 1: Hybrid Root Check (SMA-TOPO-001) ---
        // A workspace must never conflate root package code `src/` with child member crates.
        if has_root_src && !discovered.is_empty() {
            self.record(
                &mut diags,
                "SMA-TOPO-001",
                &root_manifest_path,
                format!(
                    "Topological violation: Root contains both `src/` code and {} nested member packages. Repositories must strictly choose between Facade (Leaf) or Workspace (Container).",
                    discovered.len()
                ),
                Some("Extract root `src/` into a dedicated crate under a container directory (e.g. `crates/` or `<domain>/`).".to_string()),
            );
        }

        // --- Invariant 2: Symmetric Tree Depth Check (SMA-TOPO-002) ---
        // All member crates in a workspace must reside at the same tree depth.
        if depth_set.len() > 1 {
            let depths_str: Vec<_> = depth_set.iter().map(|d| d.to_string()).collect();
            self.record(
                &mut diags,
                "SMA-TOPO-002",
                &root_manifest_path,
                format!(
                    "Topological violation: Asymmetric package depths detected across workspace: [{}]. Packages must be placed homogeneously.",
                    depths_str.join(", ")
                ),
                Some("Align all package directories to symmetric depth (depth 2: `<category>/<package>/Cargo.toml`).".to_string()),
            );
        }

        // --- Invariant 3: Taxonomy & Directory Reflection (SMA-TOPO-003) ---
        // The package name in Cargo.toml should reflect its directory path or component name.
        for pkg in &discovered {
            if let Some(pkg_name) = &pkg.package_name {
                let folder_name = pkg
                    .manifest_path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();

                if !folder_name.is_empty() && pkg_name != folder_name {
                    // Check if folder is a suffix or part of package name
                    if !pkg_name.ends_with(folder_name) && !folder_name.ends_with(pkg_name) {
                        self.record(
                            &mut diags,
                            "SMA-TOPO-003",
                            &pkg.manifest_path,
                            format!(
                                "Topological warning: Cargo package name `{pkg_name}` diverges from directory folder name `{folder_name}`."
                            ),
                            Some(format!("Rename folder to `{pkg_name}` or align package name for architectural clarity.")),
                        );
                    }
                }
            }
        }

        TopologyReport {
            detected_kind,
            is_virtual_workspace,
            has_root_package,
            has_root_src,
            discovered_packages: discovered,
            diagnostics: diags,
        }
    }

    fn collect_manifests(
        &self,
        current_dir: &Path,
        rel_path: &Path,
        depth: usize,
        discovered: &mut Vec<DiscoveredPackage>,
        depth_set: &mut BTreeSet<usize>,
        categories: &mut BTreeMap<String, usize>,
    ) {
        if depth > 4 {
            return;
        }

        let entries = match std::fs::read_dir(current_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Ignore target, git, hidden, and build artifact folders
            if name.starts_with('.')
                || name == "target"
                || name == "node_modules"
                || name == "book"
                || name == "fuzz"
            {
                continue;
            }

            if path.is_dir() {
                let child_rel = rel_path.join(&name);
                let manifest = path.join("Cargo.toml");

                if manifest.is_file() {
                    let (pkg_name, _) = self.parse_package_name(&manifest);
                    let parent_cat = rel_path
                        .components()
                        .next()
                        .map(|c| c.as_os_str().to_string_lossy().to_string());

                    if let Some(cat) = &parent_cat {
                        *categories.entry(cat.clone()).or_insert(0) += 1;
                    }

                    depth_set.insert(depth + 1);
                    discovered.push(DiscoveredPackage {
                        manifest_path: manifest,
                        relative_path: child_rel.clone(),
                        package_name: pkg_name,
                        depth: depth + 1,
                        parent_category: parent_cat,
                    });
                } else {
                    self.collect_manifests(
                        &path,
                        &child_rel,
                        depth + 1,
                        discovered,
                        depth_set,
                        categories,
                    );
                }
            }
        }
    }

    fn parse_package_name(&self, manifest_path: &Path) -> (Option<String>, bool) {
        if let Ok(content) = std::fs::read_to_string(manifest_path) {
            let mut in_pkg = false;
            let mut name = None;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    in_pkg = trimmed == "[package]";
                    continue;
                }
                if in_pkg
                    && trimmed.starts_with("name")
                    && let Some((_, val)) = trimmed.split_once('=')
                {
                    let clean = val.trim().trim_matches('"').trim_matches('\'');
                    name = Some(clean.to_string());
                    break;
                }
            }
            (name, content.contains("[package]"))
        } else {
            (None, false)
        }
    }

    fn record(
        &self,
        diags: &mut Vec<Diagnostic>,
        code: &str,
        path: &Path,
        msg: String,
        help: Option<String>,
    ) {
        let severity = self.config.severity_for(code);
        if severity == RuleSeverity::Allow {
            return;
        }

        diags.push(Diagnostic {
            code: code.to_string(),
            file: path.to_path_buf(),
            line: 1,
            column: 1,
            message: msg,
            help,
            severity,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_facade_topology_detection() {
        let temp_dir =
            std::env::temp_dir().join(format!("stitch_topo_facade_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();
        std::fs::write(
            temp_dir.join("Cargo.toml"),
            r#"
            [package]
            name = "stitch"
            version = "0.3.0"
            edition = "2024"
        "#,
        )
        .unwrap();

        let cfg = StitchConfig::default();
        let scanner = TopologyScanner::new(&cfg, &temp_dir);
        let report = scanner.evaluate();

        assert_eq!(report.detected_kind, TopologyKind::Facade);
        assert!(report.diagnostics.is_empty());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_uniform_workspace_topology_detection() {
        let temp_dir =
            std::env::temp_dir().join(format!("stitch_topo_uniform_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let c1 = temp_dir.join("crates/core");
        let c2 = temp_dir.join("crates/macros");
        std::fs::create_dir_all(&c1).unwrap();
        std::fs::create_dir_all(&c2).unwrap();

        std::fs::write(
            temp_dir.join("Cargo.toml"),
            r#"
            [workspace]
            members = ["crates/*"]
        "#,
        )
        .unwrap();

        std::fs::write(
            c1.join("Cargo.toml"),
            r#"
            [package]
            name = "core"
            version = "0.1.0"
        "#,
        )
        .unwrap();

        std::fs::write(
            c2.join("Cargo.toml"),
            r#"
            [package]
            name = "macros"
            version = "0.1.0"
        "#,
        )
        .unwrap();

        let cfg = StitchConfig::default();
        let scanner = TopologyScanner::new(&cfg, &temp_dir);
        let report = scanner.evaluate();

        assert_eq!(report.detected_kind, TopologyKind::Uniform);
        assert!(report.diagnostics.is_empty());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_segmented_hexagonal_workspace_detection() {
        let temp_dir = std::env::temp_dir().join(format!("stitch_topo_seg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let b1 = temp_dir.join("backends/backend_standalone");
        let s1 = temp_dir.join("services/service_reports");
        std::fs::create_dir_all(&b1).unwrap();
        std::fs::create_dir_all(&s1).unwrap();

        std::fs::write(
            temp_dir.join("Cargo.toml"),
            r#"
            [workspace]
            members = ["backends/*", "services/*"]
        "#,
        )
        .unwrap();

        std::fs::write(
            b1.join("Cargo.toml"),
            r#"
            [package]
            name = "backend_standalone"
            version = "0.1.0"
        "#,
        )
        .unwrap();

        std::fs::write(
            s1.join("Cargo.toml"),
            r#"
            [package]
            name = "service_reports"
            version = "0.1.0"
        "#,
        )
        .unwrap();

        let cfg = StitchConfig::default();
        let scanner = TopologyScanner::new(&cfg, &temp_dir);
        let report = scanner.evaluate();

        assert_eq!(report.detected_kind, TopologyKind::Segmented);
        assert!(report.diagnostics.is_empty());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_hybrid_root_violation_caught() {
        let temp_dir =
            std::env::temp_dir().join(format!("stitch_topo_hybrid_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let c1 = temp_dir.join("crates/core");
        std::fs::create_dir_all(&c1).unwrap();
        std::fs::create_dir_all(temp_dir.join("src")).unwrap(); // Illegal src in workspace

        std::fs::write(
            temp_dir.join("Cargo.toml"),
            r#"
            [workspace]
            members = ["crates/*"]
        "#,
        )
        .unwrap();

        std::fs::write(
            c1.join("Cargo.toml"),
            r#"
            [package]
            name = "core"
            version = "0.1.0"
        "#,
        )
        .unwrap();

        let cfg = StitchConfig::default();
        let scanner = TopologyScanner::new(&cfg, &temp_dir);
        let report = scanner.evaluate();

        assert_eq!(report.detected_kind, TopologyKind::Malformed);
        assert!(report.diagnostics.iter().any(|d| d.code == "SMA-TOPO-001"));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_asymmetric_depth_violation_caught() {
        let temp_dir =
            std::env::temp_dir().join(format!("stitch_topo_asym_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let shallow = temp_dir.join("engine"); // Depth 1
        let deep = temp_dir.join("backends/standalone"); // Depth 2
        std::fs::create_dir_all(&shallow).unwrap();
        std::fs::create_dir_all(&deep).unwrap();

        std::fs::write(
            temp_dir.join("Cargo.toml"),
            r#"
            [workspace]
            members = ["engine", "backends/*"]
        "#,
        )
        .unwrap();

        std::fs::write(
            shallow.join("Cargo.toml"),
            r#"
            [package]
            name = "engine"
            version = "0.1.0"
        "#,
        )
        .unwrap();

        std::fs::write(
            deep.join("Cargo.toml"),
            r#"
            [package]
            name = "standalone"
            version = "0.1.0"
        "#,
        )
        .unwrap();

        let cfg = StitchConfig::default();
        let scanner = TopologyScanner::new(&cfg, &temp_dir);
        let report = scanner.evaluate();

        assert_eq!(report.detected_kind, TopologyKind::Malformed);
        assert!(
            report.diagnostics.iter().any(|d| d.code == "SMA-TOPO-002"),
            "Expected SMA-TOPO-002 Asymmetric Depth violation"
        );
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_taxonomy_mismatch_warning_caught() {
        let temp_dir =
            std::env::temp_dir().join(format!("stitch_topo_taxo_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let pkg_dir = temp_dir.join("crates/audio_engine");
        std::fs::create_dir_all(&pkg_dir).unwrap();

        std::fs::write(
            temp_dir.join("Cargo.toml"),
            r#"
            [workspace]
            members = ["crates/*"]
        "#,
        )
        .unwrap();

        // Cargo package name has zero relation to folder name "audio_engine"
        std::fs::write(
            pkg_dir.join("Cargo.toml"),
            r#"
            [package]
            name = "totally_unrelated_renderer"
            version = "0.1.0"
        "#,
        )
        .unwrap();

        let cfg = StitchConfig::default();
        let scanner = TopologyScanner::new(&cfg, &temp_dir);
        let report = scanner.evaluate();

        assert!(
            report.diagnostics.iter().any(|d| d.code == "SMA-TOPO-003"),
            "Expected SMA-TOPO-003 Taxonomy Mismatch warning"
        );
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
