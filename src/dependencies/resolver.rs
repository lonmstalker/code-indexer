//! Dependency resolver trait and registry.

use std::collections::HashMap;
use std::path::Path;

use crate::error::Result;

use super::{Dependency, Ecosystem, ProjectInfo};

/// Trait for resolving dependencies in a specific ecosystem.
pub trait DependencyResolver: Send + Sync {
    /// Returns the ecosystem this resolver handles.
    fn ecosystem(&self) -> Ecosystem;

    /// Returns the manifest file names this resolver can parse.
    fn manifest_names(&self) -> &[&str];

    /// Checks if this resolver can handle the given path.
    fn can_handle(&self, path: &Path) -> bool {
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            self.manifest_names().contains(&file_name)
        } else {
            false
        }
    }

    /// Parses a manifest file and returns project information.
    fn parse_manifest(&self, path: &Path) -> Result<ProjectInfo>;

    /// Locates the source code for a dependency.
    /// Returns the path to the source directory if found.
    fn locate_sources(&self, dep: &Dependency) -> Result<Option<String>>;

    /// Resolves all dependencies for a project, populating source paths.
    fn resolve_sources(&self, project: &mut ProjectInfo) -> Result<()> {
        for dep in &mut project.dependencies {
            if dep.source_path.is_none() {
                if let Ok(Some(path)) = self.locate_sources(dep) {
                    dep.source_path = Some(path);
                }
            }
        }
        Ok(())
    }
}

/// Registry for dependency resolvers.
pub struct DependencyRegistry {
    resolvers: HashMap<Ecosystem, Box<dyn DependencyResolver>>,
}

impl DependencyRegistry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self {
            resolvers: HashMap::new(),
        }
    }

    /// Creates a registry with all built-in resolvers.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(super::cargo::CargoResolver::new()));
        registry.register(Box::new(super::npm::NpmResolver::new()));
        registry
    }

    /// Registers a resolver for an ecosystem.
    pub fn register(&mut self, resolver: Box<dyn DependencyResolver>) {
        self.resolvers.insert(resolver.ecosystem(), resolver);
    }

    /// Gets a resolver for an ecosystem.
    pub fn get(&self, ecosystem: Ecosystem) -> Option<&dyn DependencyResolver> {
        self.resolvers.get(&ecosystem).map(|r| r.as_ref())
    }

    /// Finds a resolver that can handle the given manifest path.
    pub fn find_for_manifest(&self, path: &Path) -> Option<&dyn DependencyResolver> {
        self.resolvers
            .values()
            .find(|r| r.can_handle(path))
            .map(|r| r.as_ref())
    }

    /// Detects the ecosystem from a directory by looking for manifest files.
    pub fn detect_ecosystem(&self, dir: &Path) -> Option<Ecosystem> {
        for (ecosystem, resolver) in &self.resolvers {
            for manifest_name in resolver.manifest_names() {
                if dir.join(manifest_name).exists() {
                    return Some(*ecosystem);
                }
            }
        }
        None
    }

    /// Parses a manifest file using the appropriate resolver.
    pub fn parse_manifest(&self, path: &Path) -> Result<ProjectInfo> {
        if let Some(resolver) = self.find_for_manifest(path) {
            resolver.parse_manifest(path)
        } else {
            Err(crate::error::IndexerError::Parse(format!(
                "No resolver found for manifest: {}",
                path.display()
            )))
        }
    }

    /// Returns all registered ecosystems.
    pub fn ecosystems(&self) -> Vec<Ecosystem> {
        self.resolvers.keys().copied().collect()
    }
}

impl Default for DependencyRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let registry = DependencyRegistry::new();
        assert!(registry.ecosystems().is_empty());
    }

    #[test]
    fn test_registry_with_defaults() {
        let registry = DependencyRegistry::with_defaults();
        assert!(registry.get(Ecosystem::Cargo).is_some());
        assert!(registry.get(Ecosystem::Npm).is_some());
    }

    #[test]
    fn test_registry_detect_ecosystem() {
        use std::fs;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let registry = DependencyRegistry::with_defaults();

        // No manifest - no ecosystem
        assert!(registry.detect_ecosystem(dir.path()).is_none());

        // Create Cargo.toml
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        assert_eq!(
            registry.detect_ecosystem(dir.path()),
            Some(Ecosystem::Cargo)
        );
    }
}
