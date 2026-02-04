//! Cargo/Rust dependency resolver.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{IndexerError, Result};

use super::resolver::DependencyResolver;
use super::{Dependency, Ecosystem, ProjectInfo};

/// Resolver for Rust/Cargo dependencies.
pub struct CargoResolver {
    /// Cached cargo registry path
    cargo_registry_path: Option<PathBuf>,
}

impl CargoResolver {
    pub fn new() -> Self {
        Self {
            cargo_registry_path: Self::find_cargo_registry(),
        }
    }

    /// Finds the cargo registry source directory.
    fn find_cargo_registry() -> Option<PathBuf> {
        // Check CARGO_HOME environment variable first
        if let Ok(cargo_home) = std::env::var("CARGO_HOME") {
            let registry_src = PathBuf::from(cargo_home).join("registry/src");
            if registry_src.exists() {
                return Some(registry_src);
            }
        }

        // Fall back to default location
        if let Some(home) = dirs_home() {
            let registry_src = home.join(".cargo/registry/src");
            if registry_src.exists() {
                return Some(registry_src);
            }
        }

        None
    }

    /// Finds the source directory for a crate in the cargo registry.
    fn find_crate_source(&self, name: &str, version: &str) -> Option<PathBuf> {
        let registry_src = self.cargo_registry_path.as_ref()?;

        // The registry has subdirectories like "index.crates.io-{hash}"
        // We need to search in all of them
        if let Ok(entries) = fs::read_dir(registry_src) {
            for entry in entries.flatten() {
                let index_dir = entry.path();
                if index_dir.is_dir() {
                    // Look for {name}-{version} directory
                    let crate_dir = index_dir.join(format!("{}-{}", name, version));
                    if crate_dir.exists() && crate_dir.is_dir() {
                        return Some(crate_dir);
                    }
                }
            }
        }

        None
    }

    /// Parses Cargo.toml content.
    fn parse_cargo_toml(&self, content: &str, manifest_path: &Path) -> Result<ProjectInfo> {
        let toml_value: toml::Value = content
            .parse()
            .map_err(|e: toml::de::Error| IndexerError::Parse(format!("Invalid Cargo.toml: {}", e)))?;

        let package = toml_value
            .get("package")
            .ok_or_else(|| IndexerError::Parse("Missing [package] section".to_string()))?;

        let name = package
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| IndexerError::Parse("Missing package name".to_string()))?
            .to_string();

        let version = package
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut project = ProjectInfo::new(name, Ecosystem::Cargo, manifest_path.to_string_lossy());

        if let Some(v) = version {
            project = project.with_version(v);
        }

        // Parse dependencies
        let mut deps = Vec::new();

        if let Some(dependencies) = toml_value.get("dependencies") {
            deps.extend(self.parse_dependencies_table(dependencies, false)?);
        }

        if let Some(dev_dependencies) = toml_value.get("dev-dependencies") {
            deps.extend(self.parse_dependencies_table(dev_dependencies, true)?);
        }

        if let Some(build_dependencies) = toml_value.get("build-dependencies") {
            deps.extend(self.parse_dependencies_table(build_dependencies, true)?);
        }

        project.dependencies = deps;
        Ok(project)
    }

    /// Parses a dependencies table from Cargo.toml.
    fn parse_dependencies_table(
        &self,
        table: &toml::Value,
        is_dev: bool,
    ) -> Result<Vec<Dependency>> {
        let mut deps = Vec::new();

        if let Some(map) = table.as_table() {
            for (name, value) in map {
                let version = match value {
                    // Simple version string: dependency = "1.0"
                    toml::Value::String(v) => v.clone(),
                    // Table with version: dependency = { version = "1.0", features = [...] }
                    toml::Value::Table(t) => t
                        .get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("*")
                        .to_string(),
                    _ => "*".to_string(),
                };

                let mut dep = Dependency::new(name, version, Ecosystem::Cargo);
                dep.is_dev = is_dev;
                deps.push(dep);
            }
        }

        Ok(deps)
    }

    /// Parses Cargo.lock to get exact versions.
    fn parse_cargo_lock(&self, content: &str) -> Result<HashMap<String, String>> {
        let toml_value: toml::Value = content
            .parse()
            .map_err(|e: toml::de::Error| IndexerError::Parse(format!("Invalid Cargo.lock: {}", e)))?;

        let mut versions = HashMap::new();

        if let Some(packages) = toml_value.get("package").and_then(|p| p.as_array()) {
            for package in packages {
                if let (Some(name), Some(version)) = (
                    package.get("name").and_then(|v| v.as_str()),
                    package.get("version").and_then(|v| v.as_str()),
                ) {
                    versions.insert(name.to_string(), version.to_string());
                }
            }
        }

        Ok(versions)
    }

    /// Updates dependency versions from Cargo.lock.
    fn update_versions_from_lock(
        &self,
        project: &mut ProjectInfo,
        lock_path: &Path,
    ) -> Result<()> {
        if lock_path.exists() {
            let content = fs::read_to_string(lock_path)?;
            let versions = self.parse_cargo_lock(&content)?;

            for dep in &mut project.dependencies {
                if let Some(exact_version) = versions.get(&dep.name) {
                    dep.version = exact_version.clone();
                }
            }
        }
        Ok(())
    }
}

impl Default for CargoResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyResolver for CargoResolver {
    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Cargo
    }

    fn manifest_names(&self) -> &[&str] {
        &["Cargo.toml"]
    }

    fn parse_manifest(&self, path: &Path) -> Result<ProjectInfo> {
        let content = fs::read_to_string(path)?;
        let mut project = self.parse_cargo_toml(&content, path)?;

        // Try to get exact versions from Cargo.lock
        let lock_path = path.with_file_name("Cargo.lock");
        let _ = self.update_versions_from_lock(&mut project, &lock_path);

        // Resolve source paths
        self.resolve_sources(&mut project)?;

        Ok(project)
    }

    fn locate_sources(&self, dep: &Dependency) -> Result<Option<String>> {
        if dep.ecosystem != Ecosystem::Cargo {
            return Ok(None);
        }

        // Try to find exact version first
        if let Some(path) = self.find_crate_source(&dep.name, &dep.version) {
            return Ok(Some(path.to_string_lossy().to_string()));
        }

        // If version contains semver operators, try without them
        let version = dep
            .version
            .trim_start_matches('^')
            .trim_start_matches('~')
            .trim_start_matches('=')
            .trim_start_matches('>')
            .trim_start_matches('<');

        if version != dep.version {
            if let Some(path) = self.find_crate_source(&dep.name, version) {
                return Ok(Some(path.to_string_lossy().to_string()));
            }
        }

        Ok(None)
    }
}

/// Gets the home directory.
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_cargo_toml() {
        let resolver = CargoResolver::new();
        let content = r#"
[package]
name = "test-project"
version = "0.1.0"

[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["full"] }

[dev-dependencies]
tempfile = "3.0"
"#;

        let project = resolver
            .parse_cargo_toml(content, Path::new("Cargo.toml"))
            .unwrap();

        assert_eq!(project.name, "test-project");
        assert_eq!(project.version, Some("0.1.0".to_string()));
        assert_eq!(project.ecosystem, Ecosystem::Cargo);
        assert_eq!(project.dependencies.len(), 3);

        let serde = project.dependencies.iter().find(|d| d.name == "serde");
        assert!(serde.is_some());
        assert_eq!(serde.unwrap().version, "1.0");
        assert!(!serde.unwrap().is_dev);

        let tempfile = project.dependencies.iter().find(|d| d.name == "tempfile");
        assert!(tempfile.is_some());
        assert!(tempfile.unwrap().is_dev);
    }

    #[test]
    fn test_parse_cargo_lock() {
        let resolver = CargoResolver::new();
        let content = r#"
[[package]]
name = "serde"
version = "1.0.203"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "tokio"
version = "1.38.0"
"#;

        let versions = resolver.parse_cargo_lock(content).unwrap();
        assert_eq!(versions.get("serde"), Some(&"1.0.203".to_string()));
        assert_eq!(versions.get("tokio"), Some(&"1.38.0".to_string()));
    }

    #[test]
    fn test_resolver_ecosystem() {
        let resolver = CargoResolver::new();
        assert_eq!(resolver.ecosystem(), Ecosystem::Cargo);
        assert_eq!(resolver.manifest_names(), &["Cargo.toml"]);
    }

    #[test]
    fn test_can_handle() {
        let resolver = CargoResolver::new();
        assert!(resolver.can_handle(Path::new("Cargo.toml")));
        assert!(resolver.can_handle(Path::new("/path/to/Cargo.toml")));
        assert!(!resolver.can_handle(Path::new("package.json")));
    }
}
