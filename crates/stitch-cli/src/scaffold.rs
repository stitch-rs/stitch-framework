//! Template Scaffolding Engine for SMA Repositories & Components (`stitch new`, `stitch add`).

use std::path::{Path, PathBuf};

/// Topology templates supported by `stitch new`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTemplate {
    Facade,
    Uniform,
    Segmented,
}

impl std::str::FromStr for ProjectTemplate {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "facade" | "sdk" | "lib" => Ok(Self::Facade),
            "uniform" | "workspace" | "crates" => Ok(Self::Uniform),
            "segmented" | "hexagonal" | "engine" => Ok(Self::Segmented),
            other => Err(format!(
                "Unknown template `{other}`. Allowed: facade, uniform, segmented"
            )),
        }
    }
}

pub struct ScaffoldEngine;

impl ScaffoldEngine {
    /// Generates a new SMA repository based on the specified topology template.
    pub fn create_project(
        target_dir: &Path,
        name: &str,
        template: ProjectTemplate,
        org: Option<&str>,
    ) -> Result<PathBuf, String> {
        let project_dir = target_dir.join(name);
        if project_dir.exists() {
            return Err(format!(
                "Target directory `{}` already exists.",
                project_dir.display()
            ));
        }

        std::fs::create_dir_all(&project_dir)
            .map_err(|e| format!("Failed to create `{}`: {e}", project_dir.display()))?;

        // 1. .gitignore
        std::fs::write(
            project_dir.join(".gitignore"),
            "/target\n**/*.rs.bk\nCargo.lock\n",
        )
        .map_err(|e| e.to_string())?;

        // 2. stitch.toml
        let org_str = org.unwrap_or(name);
        let topo_str = match template {
            ProjectTemplate::Facade => "facade",
            ProjectTemplate::Uniform => "uniform",
            ProjectTemplate::Segmented => "segmented",
        };
        let stitch_toml = format!(
            r#"[workspace]
schema_version = "1.0.0"
root = "."
topology = "{topo_str}"
organization = "{org_str}"

[scrooge]
cache_line_bytes = 64
max_padding_waste_pct = 0.05
"#
        );
        std::fs::write(project_dir.join("stitch.toml"), stitch_toml).map_err(|e| e.to_string())?;

        // 3. Topology-specific layout & Cargo.toml
        match template {
            ProjectTemplate::Facade => {
                let src_dir = project_dir.join("src");
                std::fs::create_dir_all(&src_dir).map_err(|e| e.to_string())?;
                let cargo_toml = format!(
                    r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
authors = ["{org_str} Contributors"]
description = "High-performance SMA Facade"

[dependencies]
stitch-core = {{ version = "0.3", default-features = false }}
stitch-macros = {{ version = "0.3" }}
"#
                );
                std::fs::write(project_dir.join("Cargo.toml"), cargo_toml)
                    .map_err(|e| e.to_string())?;

                let lib_rs = r#"//! High-performance SMA Facade crate.

#![no_std]

pub use stitch_core::*;
pub use stitch_macros::*;

pub mod prelude {
    pub use stitch_core::prelude::*;
    pub use stitch_macros::{pipeline, blackboard, token, id};
}
"#;
                std::fs::write(src_dir.join("lib.rs"), lib_rs).map_err(|e| e.to_string())?;
            }
            ProjectTemplate::Uniform => {
                let crates_dir = project_dir.join("crates");
                std::fs::create_dir_all(&crates_dir).map_err(|e| e.to_string())?;
                let cargo_toml = r#"[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2024"
authors = ["Contributors"]

[workspace.dependencies]
stitch-core = { version = "0.3", default-features = false }
stitch-macros = { version = "0.3" }
"#;
                std::fs::write(project_dir.join("Cargo.toml"), cargo_toml)
                    .map_err(|e| e.to_string())?;
            }
            ProjectTemplate::Segmented => {
                let cargo_toml = r#"[workspace]
members = [
    "engine/*",
    "services/*",
    "hosts/*",
    "adapters/*",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2024"
authors = ["Contributors"]

[workspace.dependencies]
stitch-core = { version = "0.3", default-features = false }
stitch-macros = { version = "0.3" }
"#;
                std::fs::write(project_dir.join("Cargo.toml"), cargo_toml)
                    .map_err(|e| e.to_string())?;
                for domain in &["engine", "services", "hosts", "adapters"] {
                    std::fs::create_dir_all(project_dir.join(domain)).map_err(|e| e.to_string())?;
                }
            }
        }

        Ok(project_dir)
    }

    /// Scaffolds a complete SMA component following the Component Hierarchy Standard:
    /// `mod.rs`, `context.rs`, `pipeline.rs`, `terminal.rs`, `tests.rs`.
    pub fn add_component(
        base_dir: &Path,
        name: &str,
        domain: Option<&str>,
        org: Option<&str>,
    ) -> Result<PathBuf, String> {
        let target_dir = if let Some(dom) = domain {
            base_dir.join(dom).join(name)
        } else {
            base_dir.join("crates").join(name)
        };

        if target_dir.exists() {
            return Err(format!(
                "Component directory `{}` already exists.",
                target_dir.display()
            ));
        }

        let src_dir = target_dir.join("src");
        std::fs::create_dir_all(&src_dir).map_err(|e| e.to_string())?;

        let org_prefix = org.unwrap_or("app");
        let pkg_name = if let Some(dom) = domain {
            format!("{org_prefix}-{dom}-{name}")
        } else {
            format!("{org_prefix}-{name}")
        };

        // 1. Cargo.toml
        let cargo_toml = format!(
            r#"[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2024"

[dependencies]
stitch-core = {{ version = "0.3", default-features = false }}
stitch-macros = {{ version = "0.3" }}
"#
        );
        std::fs::write(target_dir.join("Cargo.toml"), cargo_toml).map_err(|e| e.to_string())?;

        // 2. src/context.rs (Material / Blackboard, 64-byte aligned)
        let context_rs = r#"//! Material: Blackboard context and token definitions for this component.

use stitch_core::Blackboard;
use stitch_macros::{blackboard, token};

#[token]
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SessionToken(pub u64);

/// Domain Blackboard state aligned to 64-byte L1D cache boundaries.
#[blackboard]
#[repr(C, align(64))]
#[derive(Debug, Default)]
pub struct ComponentContext {
    pub session_id: u64,
    pub payload_bytes: u32,
    pub is_authorized: bool,
}
impl Blackboard for ComponentContext {}
"#;
        std::fs::write(src_dir.join("context.rs"), context_rs).map_err(|e| e.to_string())?;

        // 3. src/terminal.rs (Point of Puncture / Terminal)
        let terminal_rs = r#"//! Point of Puncture: Terminal execution handler.

use crate::context::ComponentContext;
use stitch_core::{FlowControl, Terminal};

pub struct ComponentTerminal;

impl Terminal<ComponentContext, ()> for ComponentTerminal {
    type Error = ();

    #[inline(always)]
    fn execute(&mut self, _ctx: &mut ComponentContext, _intent: ()) -> FlowControl<(), Self::Error> {
        // Execute terminal effect
        FlowControl::Proceed(())
    }
}
"#;
        std::fs::write(src_dir.join("terminal.rs"), terminal_rs).map_err(|e| e.to_string())?;

        // 4. src/pipeline.rs (Work / Monomorphic Pipeline)
        let pipeline_rs = r#"//! Work: Newtype Monomorphic Pipeline definition.

use crate::context::ComponentContext;
use crate::terminal::ComponentTerminal;
use stitch_macros::pipeline;

#[pipeline(terminal = ComponentTerminal, layers = [])]
pub struct ComponentPipeline;
"#;
        std::fs::write(src_dir.join("pipeline.rs"), pipeline_rs).map_err(|e| e.to_string())?;

        // 5. src/tests.rs (Assurance / Unit Tests)
        let tests_rs = r#"//! Component unit tests.

use super::*;
use stitch_core::FlowControl;

#[test]
fn test_component_pipeline_execution() {
    let mut ctx = context::ComponentContext::default();
    let mut pipe = pipeline::ComponentPipeline::default();
    let res = pipe.dispatch(&mut ctx, ());
    assert!(matches!(res, FlowControl::Proceed(())));
}
"#;
        std::fs::write(src_dir.join("tests.rs"), tests_rs).map_err(|e| e.to_string())?;

        // 6. src/mod.rs (lib.rs) (Point of Contact / Facade)
        let lib_rs = r#"//! Component entry point and public exports.

pub mod context;
pub mod pipeline;
pub mod terminal;

#[cfg(test)]
mod tests;

pub use context::{ComponentContext, SessionToken};
pub use pipeline::ComponentPipeline;
pub use terminal::ComponentTerminal;
"#;
        std::fs::write(src_dir.join("lib.rs"), lib_rs).map_err(|e| e.to_string())?;

        Ok(target_dir)
    }

    /// Adds a new zero-allocation SMA layer to an existing component.
    pub fn add_layer(component_dir: &Path, layer_name: &str) -> Result<PathBuf, String> {
        let layers_dir = component_dir.join("src").join("layers");
        std::fs::create_dir_all(&layers_dir).map_err(|e| e.to_string())?;

        let file_name = format!("{}.rs", layer_name.to_lowercase());
        let file_path = layers_dir.join(&file_name);
        if file_path.exists() {
            return Err(format!("Layer `{file_name}` already exists."));
        }

        let code = format!(
            r#"//! Zero-allocation SMA Middleware Layer: `{layer_name}`.

use crate::context::ComponentContext;
use stitch_core::{{FlowControl, Layer}};

#[derive(Debug, Default, Clone, Copy)]
pub struct {layer_name};

impl Layer<ComponentContext, ()> for {layer_name} {{
    type Error = ();

    #[inline(always)]
    fn on_enter(&mut self, _ctx: &mut ComponentContext, _intent: &mut ()) -> FlowControl<(), Self::Error> {{
        FlowControl::Proceed(())
    }}

    #[inline(always)]
    fn on_exit(&mut self, _ctx: &ComponentContext, _diff: ()) {{
        // Telemetry / Reactions on ascent
    }}
}}
"#
        );
        std::fs::write(&file_path, code).map_err(|e| e.to_string())?;
        Ok(file_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scaffold_facade_project() {
        let temp_dir =
            std::env::temp_dir().join(format!("stitch_scaffold_facade_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);

        let res = ScaffoldEngine::create_project(
            &temp_dir,
            "my_sdk",
            ProjectTemplate::Facade,
            Some("testorg"),
        );
        assert!(res.is_ok());
        let p = res.unwrap();

        assert!(p.join("Cargo.toml").is_file());
        assert!(p.join("stitch.toml").is_file());
        assert!(p.join("src/lib.rs").is_file());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_scaffold_component_hierarchy() {
        let temp_dir =
            std::env::temp_dir().join(format!("stitch_scaffold_comp_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);

        let res =
            ScaffoldEngine::add_component(&temp_dir, "chat", Some("services"), Some("goldsrc"));
        assert!(res.is_ok());
        let p = res.unwrap();

        assert!(p.join("Cargo.toml").is_file());
        assert!(p.join("src/lib.rs").is_file());
        assert!(p.join("src/context.rs").is_file());
        assert!(p.join("src/pipeline.rs").is_file());
        assert!(p.join("src/terminal.rs").is_file());
        assert!(p.join("src/tests.rs").is_file());

        // Add layer
        let layer_res = ScaffoldEngine::add_layer(&p, "RateLimit");
        assert!(layer_res.is_ok());
        assert!(p.join("src/layers/ratelimit.rs").is_file());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
