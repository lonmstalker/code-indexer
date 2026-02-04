//! Workspace type detection.

use std::path::Path;

use crate::error::Result;

use super::{ModuleInfo, WorkspaceInfo, WorkspaceType};

/// Detects workspace type and parses workspace configuration
pub struct WorkspaceDetector;

impl WorkspaceDetector {
    /// Detect the type of workspace at the given path
    pub fn detect(path: &Path) -> WorkspaceType {
        // Check for Cargo workspace
        let cargo_toml = path.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return WorkspaceType::CargoWorkspace;
                }
            }
        }

        // Check for Gradle multi-project
        let settings_gradle = path.join("settings.gradle");
        let settings_gradle_kts = path.join("settings.gradle.kts");
        if settings_gradle.exists() || settings_gradle_kts.exists() {
            // If settings.gradle exists, it's likely a multi-project build
            return WorkspaceType::GradleMultiProject;
        }

        // Check for NPM workspace
        let package_json = path.join("package.json");
        if package_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&package_json) {
                if content.contains("\"workspaces\"") {
                    return WorkspaceType::NpmWorkspace;
                }
            }
        }

        // Check for pnpm workspace
        let pnpm_workspace = path.join("pnpm-workspace.yaml");
        if pnpm_workspace.exists() {
            return WorkspaceType::NpmWorkspace;
        }

        // Check for Maven multi-module
        let pom_xml = path.join("pom.xml");
        if pom_xml.exists() {
            if let Ok(content) = std::fs::read_to_string(&pom_xml) {
                if content.contains("<modules>") {
                    return WorkspaceType::MavenMultiModule;
                }
            }
        }

        WorkspaceType::SingleProject
    }

    /// Parse workspace information at the given path
    pub fn parse(path: &Path) -> Result<WorkspaceInfo> {
        let workspace_type = Self::detect(path);

        match workspace_type {
            WorkspaceType::CargoWorkspace => super::cargo::parse_cargo_workspace(path),
            WorkspaceType::NpmWorkspace => super::npm::parse_npm_workspace(path),
            WorkspaceType::GradleMultiProject => super::gradle::parse_gradle_workspace(path),
            WorkspaceType::MavenMultiModule => {
                // Maven support is basic for now
                Ok(WorkspaceInfo::new(path.to_path_buf(), WorkspaceType::MavenMultiModule))
            }
            WorkspaceType::SingleProject => {
                Ok(WorkspaceInfo::new(path.to_path_buf(), WorkspaceType::SingleProject))
            }
        }
    }

    /// Check if a path is within a specific module
    pub fn find_module_for_path<'a>(
        workspace: &'a WorkspaceInfo,
        file_path: &Path,
    ) -> Option<&'a ModuleInfo> {
        let canonical_file = file_path.canonicalize().ok()?;

        for module in &workspace.modules {
            let module_path = if module.path.is_absolute() {
                module.path.clone()
            } else {
                workspace.root_path.join(&module.path)
            };

            if let Ok(canonical_module) = module_path.canonicalize() {
                if canonical_file.starts_with(&canonical_module) {
                    return Some(module);
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_single_project() {
        let temp_dir = TempDir::new().unwrap();
        assert_eq!(WorkspaceDetector::detect(temp_dir.path()), WorkspaceType::SingleProject);
    }

    #[test]
    fn test_detect_cargo_workspace() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[workspace]
members = ["crate-a", "crate-b"]
"#,
        )
        .unwrap();

        assert_eq!(WorkspaceDetector::detect(temp_dir.path()), WorkspaceType::CargoWorkspace);
    }

    #[test]
    fn test_detect_cargo_single_project() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[package]
name = "my-crate"
version = "0.1.0"
"#,
        )
        .unwrap();

        assert_eq!(WorkspaceDetector::detect(temp_dir.path()), WorkspaceType::SingleProject);
    }

    #[test]
    fn test_detect_npm_workspace() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("package.json"),
            r#"
{
    "name": "my-monorepo",
    "workspaces": ["packages/*"]
}
"#,
        )
        .unwrap();

        assert_eq!(WorkspaceDetector::detect(temp_dir.path()), WorkspaceType::NpmWorkspace);
    }

    #[test]
    fn test_detect_pnpm_workspace() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("pnpm-workspace.yaml"),
            r#"
packages:
  - 'packages/*'
"#,
        )
        .unwrap();

        assert_eq!(WorkspaceDetector::detect(temp_dir.path()), WorkspaceType::NpmWorkspace);
    }

    #[test]
    fn test_detect_gradle_multiproject() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("settings.gradle"),
            r#"
rootProject.name = 'my-project'
include ':app', ':lib'
"#,
        )
        .unwrap();

        assert_eq!(WorkspaceDetector::detect(temp_dir.path()), WorkspaceType::GradleMultiProject);
    }

    #[test]
    fn test_detect_gradle_kts_multiproject() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("settings.gradle.kts"),
            r#"
rootProject.name = "my-project"
include(":app", ":lib")
"#,
        )
        .unwrap();

        assert_eq!(WorkspaceDetector::detect(temp_dir.path()), WorkspaceType::GradleMultiProject);
    }

    #[test]
    fn test_detect_maven_multimodule() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("pom.xml"),
            r#"
<project>
    <groupId>com.example</groupId>
    <modules>
        <module>module-a</module>
        <module>module-b</module>
    </modules>
</project>
"#,
        )
        .unwrap();

        assert_eq!(WorkspaceDetector::detect(temp_dir.path()), WorkspaceType::MavenMultiModule);
    }
}
