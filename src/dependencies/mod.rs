//! Module for dependency resolution and indexing.
//!
//! This module provides support for working with project dependencies:
//! - Parsing manifest files (Cargo.toml, package.json, etc.)
//! - Locating source code of dependencies
//! - Indexing symbols from dependencies

pub mod cargo;
pub mod npm;
pub mod resolver;

use serde::{Deserialize, Serialize};

/// Supported package ecosystems
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ecosystem {
    Cargo,
    Npm,
    Maven,
    Gradle,
}

impl Ecosystem {
    pub fn as_str(&self) -> &'static str {
        match self {
            Ecosystem::Cargo => "cargo",
            Ecosystem::Npm => "npm",
            Ecosystem::Maven => "maven",
            Ecosystem::Gradle => "gradle",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "cargo" | "rust" => Some(Ecosystem::Cargo),
            "npm" | "node" | "nodejs" => Some(Ecosystem::Npm),
            "maven" => Some(Ecosystem::Maven),
            "gradle" => Some(Ecosystem::Gradle),
            _ => None,
        }
    }

    /// Returns the manifest file names for this ecosystem
    pub fn manifest_names(&self) -> &[&str] {
        match self {
            Ecosystem::Cargo => &["Cargo.toml"],
            Ecosystem::Npm => &["package.json"],
            Ecosystem::Maven => &["pom.xml"],
            Ecosystem::Gradle => &["build.gradle", "build.gradle.kts"],
        }
    }
}

/// A project dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// Name of the dependency (e.g., "serde", "react")
    pub name: String,
    /// Version of the dependency
    pub version: String,
    /// The ecosystem this dependency belongs to
    pub ecosystem: Ecosystem,
    /// Path to the source code (if available)
    pub source_path: Option<String>,
    /// Whether this is a development dependency
    pub is_dev: bool,
    /// Whether this dependency has been indexed
    pub is_indexed: bool,
}

impl Dependency {
    pub fn new(name: impl Into<String>, version: impl Into<String>, ecosystem: Ecosystem) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            ecosystem,
            source_path: None,
            is_dev: false,
            is_indexed: false,
        }
    }

    pub fn with_source_path(mut self, path: impl Into<String>) -> Self {
        self.source_path = Some(path.into());
        self
    }

    pub fn with_dev(mut self, is_dev: bool) -> Self {
        self.is_dev = is_dev;
        self
    }
}

/// Information about a project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Project name
    pub name: String,
    /// Project version
    pub version: Option<String>,
    /// The ecosystem
    pub ecosystem: Ecosystem,
    /// Path to the manifest file
    pub manifest_path: String,
    /// Project dependencies
    pub dependencies: Vec<Dependency>,
}

impl ProjectInfo {
    pub fn new(
        name: impl Into<String>,
        ecosystem: Ecosystem,
        manifest_path: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: None,
            ecosystem,
            manifest_path: manifest_path.into(),
            dependencies: Vec::new(),
        }
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn with_dependencies(mut self, deps: Vec<Dependency>) -> Self {
        self.dependencies = deps;
        self
    }
}

/// Source of a symbol (project or dependency)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SymbolSource {
    /// Symbol is from the project itself
    Project,
    /// Symbol is from a dependency
    Dependency {
        name: String,
        version: String,
        ecosystem: Ecosystem,
    },
}

impl SymbolSource {
    pub fn as_str(&self) -> &str {
        match self {
            SymbolSource::Project => "project",
            SymbolSource::Dependency { .. } => "dependency",
        }
    }

    pub fn is_project(&self) -> bool {
        matches!(self, SymbolSource::Project)
    }

    pub fn is_dependency(&self) -> bool {
        matches!(self, SymbolSource::Dependency { .. })
    }
}

impl Default for SymbolSource {
    fn default() -> Self {
        SymbolSource::Project
    }
}

// Re-export commonly used types
pub use cargo::CargoResolver;
pub use npm::NpmResolver;
pub use resolver::{DependencyRegistry, DependencyResolver};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecosystem_as_str() {
        assert_eq!(Ecosystem::Cargo.as_str(), "cargo");
        assert_eq!(Ecosystem::Npm.as_str(), "npm");
        assert_eq!(Ecosystem::Maven.as_str(), "maven");
        assert_eq!(Ecosystem::Gradle.as_str(), "gradle");
    }

    #[test]
    fn test_ecosystem_from_str() {
        assert_eq!(Ecosystem::from_str("cargo"), Some(Ecosystem::Cargo));
        assert_eq!(Ecosystem::from_str("rust"), Some(Ecosystem::Cargo));
        assert_eq!(Ecosystem::from_str("npm"), Some(Ecosystem::Npm));
        assert_eq!(Ecosystem::from_str("node"), Some(Ecosystem::Npm));
        assert_eq!(Ecosystem::from_str("maven"), Some(Ecosystem::Maven));
        assert_eq!(Ecosystem::from_str("gradle"), Some(Ecosystem::Gradle));
        assert_eq!(Ecosystem::from_str("unknown"), None);
    }

    #[test]
    fn test_ecosystem_manifest_names() {
        assert_eq!(Ecosystem::Cargo.manifest_names(), &["Cargo.toml"]);
        assert_eq!(Ecosystem::Npm.manifest_names(), &["package.json"]);
    }

    #[test]
    fn test_dependency_new() {
        let dep = Dependency::new("serde", "1.0.0", Ecosystem::Cargo);
        assert_eq!(dep.name, "serde");
        assert_eq!(dep.version, "1.0.0");
        assert_eq!(dep.ecosystem, Ecosystem::Cargo);
        assert!(dep.source_path.is_none());
        assert!(!dep.is_dev);
        assert!(!dep.is_indexed);
    }

    #[test]
    fn test_dependency_builder() {
        let dep = Dependency::new("serde", "1.0.0", Ecosystem::Cargo)
            .with_source_path("/path/to/serde")
            .with_dev(true);

        assert_eq!(dep.source_path, Some("/path/to/serde".to_string()));
        assert!(dep.is_dev);
    }

    #[test]
    fn test_project_info() {
        let deps = vec![
            Dependency::new("serde", "1.0.0", Ecosystem::Cargo),
            Dependency::new("tokio", "1.0.0", Ecosystem::Cargo).with_dev(true),
        ];

        let project = ProjectInfo::new("my-project", Ecosystem::Cargo, "Cargo.toml")
            .with_version("0.1.0")
            .with_dependencies(deps);

        assert_eq!(project.name, "my-project");
        assert_eq!(project.version, Some("0.1.0".to_string()));
        assert_eq!(project.ecosystem, Ecosystem::Cargo);
        assert_eq!(project.dependencies.len(), 2);
    }

    #[test]
    fn test_symbol_source() {
        let project = SymbolSource::Project;
        assert!(project.is_project());
        assert!(!project.is_dependency());
        assert_eq!(project.as_str(), "project");

        let dep = SymbolSource::Dependency {
            name: "serde".to_string(),
            version: "1.0.0".to_string(),
            ecosystem: Ecosystem::Cargo,
        };
        assert!(!dep.is_project());
        assert!(dep.is_dependency());
        assert_eq!(dep.as_str(), "dependency");
    }
}
