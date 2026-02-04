//! NPM/Yarn/pnpm workspace parsing.

use std::path::Path;

use crate::error::{IndexerError, Result};

use super::{ModuleInfo, ModuleType, WorkspaceInfo, WorkspaceType};

/// Parse an NPM workspace
pub fn parse_npm_workspace(path: &Path) -> Result<WorkspaceInfo> {
    let mut workspace = WorkspaceInfo::new(path.to_path_buf(), WorkspaceType::NpmWorkspace);

    // Try package.json first
    let package_json_path = path.join("package.json");
    if package_json_path.exists() {
        let content = std::fs::read_to_string(&package_json_path).map_err(|e| {
            IndexerError::FileNotFound(format!("package.json not found: {}", e))
        })?;

        let json: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
            IndexerError::Parse(format!("Failed to parse package.json: {}", e))
        })?;

        // Get workspace name
        if let Some(name) = json.get("name").and_then(|v| v.as_str()) {
            workspace.name = Some(name.to_string());
        }

        // Parse workspaces
        if let Some(workspaces) = json.get("workspaces") {
            let patterns = extract_workspace_patterns(workspaces);
            for pattern in patterns {
                let expanded = expand_npm_glob_pattern(path, &pattern);
                for pkg_path in expanded {
                    if let Some(module) = parse_npm_package(path, &pkg_path) {
                        workspace.modules.push(module);
                    }
                }
            }
        }
    }

    // Also check pnpm-workspace.yaml
    let pnpm_workspace_path = path.join("pnpm-workspace.yaml");
    if pnpm_workspace_path.exists() && workspace.modules.is_empty() {
        if let Ok(content) = std::fs::read_to_string(&pnpm_workspace_path) {
            // Simple YAML parsing for packages field
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("- '") || trimmed.starts_with("- \"") {
                    let pattern = trimmed
                        .trim_start_matches("- '")
                        .trim_start_matches("- \"")
                        .trim_end_matches('\'')
                        .trim_end_matches('"');

                    let expanded = expand_npm_glob_pattern(path, pattern);
                    for pkg_path in expanded {
                        if let Some(module) = parse_npm_package(path, &pkg_path) {
                            workspace.modules.push(module);
                        }
                    }
                }
            }
        }
    }

    Ok(workspace)
}

fn extract_workspace_patterns(workspaces: &serde_json::Value) -> Vec<String> {
    match workspaces {
        serde_json::Value::Array(arr) => {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        }
        serde_json::Value::Object(obj) => {
            // yarn workspaces format: { "packages": [...] }
            if let Some(packages) = obj.get("packages") {
                extract_workspace_patterns(packages)
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn parse_npm_package(workspace_root: &Path, pkg_path: &Path) -> Option<ModuleInfo> {
    let package_json = pkg_path.join("package.json");

    if !package_json.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&package_json).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    let name = json.get("name")?.as_str()?.to_string();

    let relative_path = pkg_path
        .strip_prefix(workspace_root)
        .ok()?
        .to_path_buf();

    let mut module = ModuleInfo::new(name, relative_path);

    // Detect language
    let has_ts_config = pkg_path.join("tsconfig.json").exists();
    let has_ts_files = std::fs::read_dir(pkg_path.join("src"))
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .any(|e| e.path().extension().map_or(false, |ext| ext == "ts" || ext == "tsx"))
        })
        .unwrap_or(false);

    if has_ts_config || has_ts_files {
        module.language = Some("typescript".to_string());
    } else {
        module.language = Some("javascript".to_string());
    }

    // Determine module type
    if json.get("bin").is_some() {
        module.module_type = Some(ModuleType::Binary);
    } else if json.get("main").is_some() || json.get("exports").is_some() {
        module.module_type = Some(ModuleType::Library);
    }

    // Parse internal dependencies
    let mut internal_deps = Vec::new();

    for dep_field in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(deps) = json.get(dep_field).and_then(|v| v.as_object()) {
            for (dep_name, dep_version) in deps {
                // Check if it's a workspace dependency
                if let Some(version) = dep_version.as_str() {
                    if version.starts_with("workspace:") || version == "*" {
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

fn expand_npm_glob_pattern(root: &Path, pattern: &str) -> Vec<std::path::PathBuf> {
    let mut results = Vec::new();

    // Handle patterns like "packages/*" or "apps/*"
    if let Some(base) = pattern.strip_suffix("/*") {
        let base_path = root.join(base);
        if let Ok(entries) = std::fs::read_dir(&base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("package.json").exists() {
                    results.push(path);
                }
            }
        }
    } else if let Some(base) = pattern.strip_suffix("/**") {
        // Recursive glob
        let base_path = root.join(base);
        collect_npm_packages(&base_path, &mut results);
    } else if !pattern.contains('*') {
        // No glob, just return the path
        let path = root.join(pattern);
        if path.join("package.json").exists() {
            results.push(path);
        }
    }

    results
}

fn collect_npm_packages(dir: &Path, results: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.join("package.json").exists() {
                    results.push(path.clone());
                }
                // Recurse (but not into node_modules)
                if path.file_name().map_or(true, |n| n != "node_modules") {
                    collect_npm_packages(&path, results);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_npm_workspace(temp_dir: &TempDir) {
        // Create root package.json
        fs::write(
            temp_dir.path().join("package.json"),
            r#"
{
    "name": "my-monorepo",
    "private": true,
    "workspaces": ["packages/*"]
}
"#,
        )
        .unwrap();

        // Create packages
        for (name, has_ts) in [("core", true), ("cli", false)] {
            let pkg_path = temp_dir.path().join("packages").join(name);
            fs::create_dir_all(pkg_path.join("src")).unwrap();

            let deps = if name == "cli" {
                r#""dependencies": { "core": "workspace:*" }"#
            } else {
                r#""main": "dist/index.js""#
            };

            fs::write(
                pkg_path.join("package.json"),
                format!(
                    r#"{{
    "name": "@monorepo/{}",
    {}
}}"#,
                    name, deps
                ),
            )
            .unwrap();

            if has_ts {
                fs::write(pkg_path.join("tsconfig.json"), "{}").unwrap();
            }
        }
    }

    #[test]
    fn test_parse_npm_workspace() {
        let temp_dir = TempDir::new().unwrap();
        create_npm_workspace(&temp_dir);

        let workspace = parse_npm_workspace(temp_dir.path()).unwrap();

        assert_eq!(workspace.workspace_type, WorkspaceType::NpmWorkspace);
        assert_eq!(workspace.name, Some("my-monorepo".to_string()));
        assert_eq!(workspace.modules.len(), 2);

        let core = workspace.get_module("@monorepo/core").unwrap();
        assert_eq!(core.language, Some("typescript".to_string()));
        assert_eq!(core.module_type, Some(ModuleType::Library));

        let cli = workspace.get_module("@monorepo/cli").unwrap();
        assert!(cli.internal_dependencies.contains(&"core".to_string()));
    }

    #[test]
    fn test_parse_pnpm_workspace() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("pnpm-workspace.yaml"),
            r#"
packages:
  - 'apps/*'
  - 'libs/*'
"#,
        )
        .unwrap();

        // Create app
        let app_path = temp_dir.path().join("apps/web");
        fs::create_dir_all(&app_path).unwrap();
        fs::write(
            app_path.join("package.json"),
            r#"{ "name": "web-app" }"#,
        )
        .unwrap();

        // Create lib
        let lib_path = temp_dir.path().join("libs/shared");
        fs::create_dir_all(&lib_path).unwrap();
        fs::write(
            lib_path.join("package.json"),
            r#"{ "name": "shared-lib", "main": "index.js" }"#,
        )
        .unwrap();

        let workspace = parse_npm_workspace(temp_dir.path()).unwrap();

        assert_eq!(workspace.modules.len(), 2);
        assert!(workspace.get_module("web-app").is_some());
        assert!(workspace.get_module("shared-lib").is_some());
    }
}
