//! Workspace support for multi-module projects.
//!
//! This module provides detection and handling of:
//! - Cargo workspaces
//! - NPM/Yarn/pnpm workspaces
//! - Gradle multi-project builds

pub mod cargo;
pub mod detector;
pub mod gradle;
pub mod npm;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use detector::WorkspaceDetector;

/// Represents a workspace containing multiple modules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    /// Root path of the workspace
    pub root_path: PathBuf,
    /// Type of workspace
    pub workspace_type: WorkspaceType,
    /// Modules in the workspace
    pub modules: Vec<ModuleInfo>,
    /// Workspace name (if available)
    pub name: Option<String>,
}

impl WorkspaceInfo {
    pub fn new(root_path: PathBuf, workspace_type: WorkspaceType) -> Self {
        Self {
            root_path,
            workspace_type,
            modules: Vec::new(),
            name: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_modules(mut self, modules: Vec<ModuleInfo>) -> Self {
        self.modules = modules;
        self
    }

    /// Get a module by name
    pub fn get_module(&self, name: &str) -> Option<&ModuleInfo> {
        self.modules.iter().find(|m| m.name == name)
    }

    /// Get all module names
    pub fn module_names(&self) -> Vec<&str> {
        self.modules.iter().map(|m| m.name.as_str()).collect()
    }
}

/// Information about a module within a workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    /// Module name
    pub name: String,
    /// Path to the module (relative to workspace root or absolute)
    pub path: PathBuf,
    /// Dependencies on other modules in the workspace
    pub internal_dependencies: Vec<String>,
    /// Primary language of the module
    pub language: Option<String>,
    /// Module type (library, binary, etc.)
    pub module_type: Option<ModuleType>,
}

impl ModuleInfo {
    pub fn new(name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            name: name.into(),
            path,
            internal_dependencies: Vec::new(),
            language: None,
            module_type: None,
        }
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    pub fn with_dependencies(mut self, deps: Vec<String>) -> Self {
        self.internal_dependencies = deps;
        self
    }

    pub fn with_module_type(mut self, module_type: ModuleType) -> Self {
        self.module_type = Some(module_type);
        self
    }
}

/// Type of workspace
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceType {
    /// Cargo workspace (Rust)
    CargoWorkspace,
    /// NPM/Yarn/pnpm workspace (JavaScript/TypeScript)
    NpmWorkspace,
    /// Gradle multi-project build (Java/Kotlin)
    GradleMultiProject,
    /// Maven multi-module project (Java)
    MavenMultiModule,
    /// Single project (not a workspace)
    SingleProject,
}

impl WorkspaceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceType::CargoWorkspace => "cargo_workspace",
            WorkspaceType::NpmWorkspace => "npm_workspace",
            WorkspaceType::GradleMultiProject => "gradle_multi_project",
            WorkspaceType::MavenMultiModule => "maven_multi_module",
            WorkspaceType::SingleProject => "single_project",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "cargo_workspace" => Some(WorkspaceType::CargoWorkspace),
            "npm_workspace" => Some(WorkspaceType::NpmWorkspace),
            "gradle_multi_project" => Some(WorkspaceType::GradleMultiProject),
            "maven_multi_module" => Some(WorkspaceType::MavenMultiModule),
            "single_project" => Some(WorkspaceType::SingleProject),
            _ => None,
        }
    }
}

/// Type of module
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleType {
    Library,
    Binary,
    Application,
    Test,
    Plugin,
    Platform,
}

impl ModuleType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModuleType::Library => "library",
            ModuleType::Binary => "binary",
            ModuleType::Application => "application",
            ModuleType::Test => "test",
            ModuleType::Plugin => "plugin",
            ModuleType::Platform => "platform",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "library" | "lib" => Some(ModuleType::Library),
            "binary" | "bin" => Some(ModuleType::Binary),
            "application" | "app" => Some(ModuleType::Application),
            "test" => Some(ModuleType::Test),
            "plugin" => Some(ModuleType::Plugin),
            "platform" => Some(ModuleType::Platform),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_type_as_str() {
        assert_eq!(WorkspaceType::CargoWorkspace.as_str(), "cargo_workspace");
        assert_eq!(WorkspaceType::NpmWorkspace.as_str(), "npm_workspace");
        assert_eq!(WorkspaceType::GradleMultiProject.as_str(), "gradle_multi_project");
        assert_eq!(WorkspaceType::SingleProject.as_str(), "single_project");
    }

    #[test]
    fn test_workspace_type_from_str() {
        assert_eq!(WorkspaceType::from_str("cargo_workspace"), Some(WorkspaceType::CargoWorkspace));
        assert_eq!(WorkspaceType::from_str("npm_workspace"), Some(WorkspaceType::NpmWorkspace));
        assert_eq!(WorkspaceType::from_str("invalid"), None);
    }

    #[test]
    fn test_workspace_info_new() {
        let workspace = WorkspaceInfo::new(PathBuf::from("/test"), WorkspaceType::CargoWorkspace);
        assert_eq!(workspace.root_path, PathBuf::from("/test"));
        assert_eq!(workspace.workspace_type, WorkspaceType::CargoWorkspace);
        assert!(workspace.modules.is_empty());
    }

    #[test]
    fn test_workspace_info_with_modules() {
        let module = ModuleInfo::new("core", PathBuf::from("core"));
        let workspace = WorkspaceInfo::new(PathBuf::from("/test"), WorkspaceType::CargoWorkspace)
            .with_modules(vec![module]);

        assert_eq!(workspace.modules.len(), 1);
        assert_eq!(workspace.get_module("core").unwrap().name, "core");
    }

    #[test]
    fn test_module_info_new() {
        let module = ModuleInfo::new("my-module", PathBuf::from("packages/my-module"))
            .with_language("rust")
            .with_dependencies(vec!["core".to_string()])
            .with_module_type(ModuleType::Library);

        assert_eq!(module.name, "my-module");
        assert_eq!(module.language, Some("rust".to_string()));
        assert_eq!(module.internal_dependencies, vec!["core"]);
        assert_eq!(module.module_type, Some(ModuleType::Library));
    }

    #[test]
    fn test_module_type_as_str() {
        assert_eq!(ModuleType::Library.as_str(), "library");
        assert_eq!(ModuleType::Binary.as_str(), "binary");
        assert_eq!(ModuleType::Application.as_str(), "application");
    }

    #[test]
    fn test_module_type_from_str() {
        assert_eq!(ModuleType::from_str("library"), Some(ModuleType::Library));
        assert_eq!(ModuleType::from_str("lib"), Some(ModuleType::Library));
        assert_eq!(ModuleType::from_str("binary"), Some(ModuleType::Binary));
        assert_eq!(ModuleType::from_str("bin"), Some(ModuleType::Binary));
    }
}
