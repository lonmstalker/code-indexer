//! Cargo workspace parsing.

use std::path::Path;

use crate::error::{IndexerError, Result};

use super::{ModuleInfo, ModuleType, WorkspaceInfo, WorkspaceType};

/// Parse a Cargo workspace
pub fn parse_cargo_workspace(path: &Path) -> Result<WorkspaceInfo> {
    let cargo_toml_path = path.join("Cargo.toml");

    let content = std::fs::read_to_string(&cargo_toml_path).map_err(|e| {
        IndexerError::FileNotFound(format!("Cargo.toml not found: {}", e))
    })?;

    let doc: toml::Value = content.parse().map_err(|e| {
        IndexerError::Parse(format!("Failed to parse Cargo.toml: {}", e))
    })?;

    let mut workspace = WorkspaceInfo::new(path.to_path_buf(), WorkspaceType::CargoWorkspace);

    // Get workspace name from root package if available
    if let Some(package) = doc.get("package") {
        if let Some(name) = package.get("name").and_then(|v| v.as_str()) {
            workspace.name = Some(name.to_string());
        }
    }

    // Parse workspace members
    if let Some(ws) = doc.get("workspace") {
        let members = ws
            .get("members")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for member_pattern in members {
            // Handle glob patterns
            if member_pattern.contains('*') {
                // Expand glob pattern
                let expanded = expand_glob_pattern(path, member_pattern);
                for member_path in expanded {
                    if let Some(module) = parse_cargo_member(path, &member_path) {
                        workspace.modules.push(module);
                    }
                }
            } else {
                let member_path = path.join(member_pattern);
                if let Some(module) = parse_cargo_member(path, &member_path) {
                    workspace.modules.push(module);
                }
            }
        }

        // Parse default-members if available
        if let Some(default_members) = ws.get("default-members").and_then(|v| v.as_array()) {
            let default_names: Vec<&str> = default_members
                .iter()
                .filter_map(|v| v.as_str())
                .collect();

            // Mark default members
            for module in &mut workspace.modules {
                if default_names.contains(&module.name.as_str()) {
                    // Could add a flag here
                }
            }
        }
    }

    Ok(workspace)
}

fn parse_cargo_member(workspace_root: &Path, member_path: &Path) -> Option<ModuleInfo> {
    let cargo_toml = member_path.join("Cargo.toml");

    if !cargo_toml.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&cargo_toml).ok()?;
    let doc: toml::Value = content.parse().ok()?;

    let package = doc.get("package")?;
    let name = package.get("name")?.as_str()?.to_string();

    let relative_path = member_path
        .strip_prefix(workspace_root)
        .ok()?
        .to_path_buf();

    let mut module = ModuleInfo::new(name, relative_path)
        .with_language("rust");

    // Determine module type
    let has_lib = member_path.join("src/lib.rs").exists();
    let has_main = member_path.join("src/main.rs").exists();

    if has_lib && has_main {
        module.module_type = Some(ModuleType::Application);
    } else if has_main {
        module.module_type = Some(ModuleType::Binary);
    } else if has_lib {
        module.module_type = Some(ModuleType::Library);
    }

    // Parse internal dependencies
    let mut internal_deps = Vec::new();

    if let Some(deps) = doc.get("dependencies") {
        if let Some(table) = deps.as_table() {
            for (dep_name, dep_value) in table {
                // Check if it's a path dependency (internal)
                if let Some(obj) = dep_value.as_table() {
                    if obj.contains_key("path") {
                        internal_deps.push(dep_name.clone());
                    }
                }
            }
        }
    }

    if !internal_deps.is_empty() {
        module.internal_dependencies = internal_deps;
    }

    Some(module)
}

fn expand_glob_pattern(root: &Path, pattern: &str) -> Vec<std::path::PathBuf> {
    let mut results = Vec::new();

    // Simple glob expansion for patterns like "crates/*" or "packages/*"
    if let Some(base) = pattern.strip_suffix("/*") {
        let base_path = root.join(base);
        if let Ok(entries) = std::fs::read_dir(&base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("Cargo.toml").exists() {
                    results.push(path);
                }
            }
        }
    } else if let Some(base) = pattern.strip_suffix("/**") {
        // Recursive glob
        let base_path = root.join(base);
        collect_cargo_projects(&base_path, &mut results);
    } else {
        // No glob, just return the path
        results.push(root.join(pattern));
    }

    results
}

fn collect_cargo_projects(dir: &Path, results: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.join("Cargo.toml").exists() {
                    results.push(path.clone());
                }
                // Recurse
                collect_cargo_projects(&path, results);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_cargo_workspace(temp_dir: &TempDir) {
        // Create root Cargo.toml
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[workspace]
members = ["crates/core", "crates/cli"]

[workspace.package]
version = "0.1.0"
"#,
        )
        .unwrap();

        // Create core crate
        let core_path = temp_dir.path().join("crates/core");
        fs::create_dir_all(&core_path).unwrap();
        fs::write(
            core_path.join("Cargo.toml"),
            r#"
[package]
name = "core"
version = "0.1.0"
"#,
        )
        .unwrap();
        fs::create_dir_all(core_path.join("src")).unwrap();
        fs::write(core_path.join("src/lib.rs"), "").unwrap();

        // Create cli crate
        let cli_path = temp_dir.path().join("crates/cli");
        fs::create_dir_all(&cli_path).unwrap();
        fs::write(
            cli_path.join("Cargo.toml"),
            r#"
[package]
name = "cli"
version = "0.1.0"

[dependencies]
core = { path = "../core" }
"#,
        )
        .unwrap();
        fs::create_dir_all(cli_path.join("src")).unwrap();
        fs::write(cli_path.join("src/main.rs"), "fn main() {}").unwrap();
    }

    #[test]
    fn test_parse_cargo_workspace() {
        let temp_dir = TempDir::new().unwrap();
        create_cargo_workspace(&temp_dir);

        let workspace = parse_cargo_workspace(temp_dir.path()).unwrap();

        assert_eq!(workspace.workspace_type, WorkspaceType::CargoWorkspace);
        assert_eq!(workspace.modules.len(), 2);

        let core = workspace.get_module("core").unwrap();
        assert_eq!(core.language, Some("rust".to_string()));
        assert_eq!(core.module_type, Some(ModuleType::Library));

        let cli = workspace.get_module("cli").unwrap();
        assert_eq!(cli.module_type, Some(ModuleType::Binary));
        assert!(cli.internal_dependencies.contains(&"core".to_string()));
    }

    #[test]
    fn test_parse_cargo_workspace_with_glob() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[workspace]
members = ["packages/*"]
"#,
        )
        .unwrap();

        // Create packages
        for name in ["pkg-a", "pkg-b"] {
            let pkg_path = temp_dir.path().join("packages").join(name);
            fs::create_dir_all(pkg_path.join("src")).unwrap();
            fs::write(
                pkg_path.join("Cargo.toml"),
                format!(
                    r#"
[package]
name = "{}"
version = "0.1.0"
"#,
                    name
                ),
            )
            .unwrap();
            fs::write(pkg_path.join("src/lib.rs"), "").unwrap();
        }

        let workspace = parse_cargo_workspace(temp_dir.path()).unwrap();
        assert_eq!(workspace.modules.len(), 2);

        let names: Vec<_> = workspace.module_names();
        assert!(names.contains(&"pkg-a"));
        assert!(names.contains(&"pkg-b"));
    }
}
