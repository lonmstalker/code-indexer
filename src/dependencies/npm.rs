//! NPM/Node.js dependency resolver.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{IndexerError, Result};

use super::resolver::DependencyResolver;
use super::{Dependency, Ecosystem, ProjectInfo};

/// Resolver for NPM/Node.js dependencies.
pub struct NpmResolver;

impl NpmResolver {
    pub fn new() -> Self {
        Self
    }

    /// Finds the node_modules directory.
    fn find_node_modules(manifest_path: &Path) -> Option<PathBuf> {
        let parent = manifest_path.parent()?;
        let node_modules = parent.join("node_modules");
        if node_modules.exists() && node_modules.is_dir() {
            Some(node_modules)
        } else {
            None
        }
    }

    /// Finds the source directory for a package in node_modules.
    fn find_package_source(node_modules: &Path, name: &str) -> Option<PathBuf> {
        // Handle scoped packages like @types/react
        let package_dir = if name.starts_with('@') {
            // Scoped package: @scope/name -> node_modules/@scope/name
            node_modules.join(name)
        } else {
            node_modules.join(name)
        };

        if package_dir.exists() && package_dir.is_dir() {
            Some(package_dir)
        } else {
            None
        }
    }

    /// Parses package.json content.
    fn parse_package_json(&self, content: &str, manifest_path: &Path) -> Result<ProjectInfo> {
        let pkg: PackageJson = serde_json::from_str(content)
            .map_err(|e| IndexerError::Parse(format!("Invalid package.json: {}", e)))?;

        let name = pkg
            .name
            .ok_or_else(|| IndexerError::Parse("Missing package name".to_string()))?;

        let mut project =
            ProjectInfo::new(name, Ecosystem::Npm, manifest_path.to_string_lossy());

        if let Some(v) = pkg.version {
            project = project.with_version(v);
        }

        // Parse dependencies
        let mut deps = Vec::new();

        if let Some(dependencies) = pkg.dependencies {
            for (name, version) in dependencies {
                let dep = Dependency::new(name, version, Ecosystem::Npm).with_dev(false);
                deps.push(dep);
            }
        }

        if let Some(dev_dependencies) = pkg.dev_dependencies {
            for (name, version) in dev_dependencies {
                let dep = Dependency::new(name, version, Ecosystem::Npm).with_dev(true);
                deps.push(dep);
            }
        }

        if let Some(peer_dependencies) = pkg.peer_dependencies {
            for (name, version) in peer_dependencies {
                let dep = Dependency::new(name, version, Ecosystem::Npm).with_dev(false);
                deps.push(dep);
            }
        }

        if let Some(optional_dependencies) = pkg.optional_dependencies {
            for (name, version) in optional_dependencies {
                let dep = Dependency::new(name, version, Ecosystem::Npm).with_dev(false);
                deps.push(dep);
            }
        }

        project.dependencies = deps;
        Ok(project)
    }

    /// Gets the installed version of a package from its package.json.
    fn get_installed_version(node_modules: &Path, name: &str) -> Option<String> {
        let package_json = if name.starts_with('@') {
            node_modules.join(name).join("package.json")
        } else {
            node_modules.join(name).join("package.json")
        };

        if let Ok(content) = fs::read_to_string(&package_json) {
            if let Ok(pkg) = serde_json::from_str::<PackageJson>(&content) {
                return pkg.version;
            }
        }
        None
    }
}

impl Default for NpmResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyResolver for NpmResolver {
    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Npm
    }

    fn manifest_names(&self) -> &[&str] {
        &["package.json"]
    }

    fn parse_manifest(&self, path: &Path) -> Result<ProjectInfo> {
        let content = fs::read_to_string(path)?;
        let mut project = self.parse_package_json(&content, path)?;

        // Resolve source paths and update versions from installed packages
        if let Some(node_modules) = Self::find_node_modules(path) {
            for dep in &mut project.dependencies {
                if let Some(package_dir) = Self::find_package_source(&node_modules, &dep.name) {
                    dep.source_path = Some(package_dir.to_string_lossy().to_string());

                    // Get actual installed version
                    if let Some(installed_version) =
                        Self::get_installed_version(&node_modules, &dep.name)
                    {
                        dep.version = installed_version;
                    }
                }
            }
        }

        Ok(project)
    }

    fn locate_sources(&self, dep: &Dependency) -> Result<Option<String>> {
        if dep.ecosystem != Ecosystem::Npm {
            return Ok(None);
        }

        // For NPM, we need the project path to find node_modules
        // This method is less useful for NPM since node_modules is project-local
        // The source paths are typically resolved during parse_manifest

        Ok(dep.source_path.clone())
    }
}

/// Minimal representation of package.json
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageJson {
    name: Option<String>,
    version: Option<String>,
    #[serde(default)]
    dependencies: Option<HashMap<String, String>>,
    #[serde(default)]
    dev_dependencies: Option<HashMap<String, String>>,
    #[serde(default)]
    peer_dependencies: Option<HashMap<String, String>>,
    #[serde(default)]
    optional_dependencies: Option<HashMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_package_json() {
        let resolver = NpmResolver::new();
        let content = r#"
{
    "name": "test-project",
    "version": "1.0.0",
    "dependencies": {
        "react": "^18.0.0",
        "axios": "1.6.0"
    },
    "devDependencies": {
        "typescript": "^5.0.0",
        "@types/react": "^18.0.0"
    }
}
"#;

        let project = resolver
            .parse_package_json(content, Path::new("package.json"))
            .unwrap();

        assert_eq!(project.name, "test-project");
        assert_eq!(project.version, Some("1.0.0".to_string()));
        assert_eq!(project.ecosystem, Ecosystem::Npm);
        assert_eq!(project.dependencies.len(), 4);

        let react = project.dependencies.iter().find(|d| d.name == "react");
        assert!(react.is_some());
        assert_eq!(react.unwrap().version, "^18.0.0");
        assert!(!react.unwrap().is_dev);

        let typescript = project
            .dependencies
            .iter()
            .find(|d| d.name == "typescript");
        assert!(typescript.is_some());
        assert!(typescript.unwrap().is_dev);

        // Check scoped package
        let types_react = project
            .dependencies
            .iter()
            .find(|d| d.name == "@types/react");
        assert!(types_react.is_some());
    }

    #[test]
    fn test_resolver_ecosystem() {
        let resolver = NpmResolver::new();
        assert_eq!(resolver.ecosystem(), Ecosystem::Npm);
        assert_eq!(resolver.manifest_names(), &["package.json"]);
    }

    #[test]
    fn test_can_handle() {
        let resolver = NpmResolver::new();
        assert!(resolver.can_handle(Path::new("package.json")));
        assert!(resolver.can_handle(Path::new("/path/to/package.json")));
        assert!(!resolver.can_handle(Path::new("Cargo.toml")));
    }

    #[test]
    fn test_parse_minimal_package_json() {
        let resolver = NpmResolver::new();
        let content = r#"
{
    "name": "minimal"
}
"#;

        let project = resolver
            .parse_package_json(content, Path::new("package.json"))
            .unwrap();

        assert_eq!(project.name, "minimal");
        assert!(project.version.is_none());
        assert!(project.dependencies.is_empty());
    }
}
