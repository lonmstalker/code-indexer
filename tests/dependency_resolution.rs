//! Integration tests for dependency resolution.
//!
//! These tests verify the parsing and resolution of dependencies
//! from Cargo.toml, package.json, and other manifest files.

use tempfile::TempDir;

use code_indexer::dependencies::{DependencyRegistry, Ecosystem, ProjectInfo};

// ============================================================================
// Test Helpers
// ============================================================================

/// Creates a temp directory with a Cargo.toml
fn create_cargo_project(name: &str, dependencies: &[(&str, &str)]) -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cargo_toml = temp_dir.path().join("Cargo.toml");

    let deps_section = if dependencies.is_empty() {
        String::new()
    } else {
        let deps: Vec<String> = dependencies
            .iter()
            .map(|(n, v)| format!("{} = \"{}\"", n, v))
            .collect();
        format!("\n[dependencies]\n{}", deps.join("\n"))
    };

    let content = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"
{}"#,
        name, deps_section
    );

    std::fs::write(&cargo_toml, content).expect("Failed to write Cargo.toml");
    temp_dir
}

/// Creates a temp directory with a package.json
fn create_npm_project(name: &str, dependencies: &[(&str, &str)], dev_deps: &[(&str, &str)]) -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let package_json = temp_dir.path().join("package.json");

    let deps_obj = if dependencies.is_empty() {
        "{}".to_string()
    } else {
        let entries: Vec<String> = dependencies
            .iter()
            .map(|(n, v)| format!("    \"{}\": \"{}\"", n, v))
            .collect();
        format!("{{\n{}\n  }}", entries.join(",\n"))
    };

    let dev_deps_obj = if dev_deps.is_empty() {
        "{}".to_string()
    } else {
        let entries: Vec<String> = dev_deps
            .iter()
            .map(|(n, v)| format!("    \"{}\": \"{}\"", n, v))
            .collect();
        format!("{{\n{}\n  }}", entries.join(",\n"))
    };

    let content = format!(
        r#"{{
  "name": "{}",
  "version": "1.0.0",
  "dependencies": {},
  "devDependencies": {}
}}"#,
        name, deps_obj, dev_deps_obj
    );

    std::fs::write(&package_json, content).expect("Failed to write package.json");
    temp_dir
}

// ============================================================================
// Cargo/Rust Dependency Tests
// ============================================================================

mod cargo_dependencies {
    use super::*;

    #[test]
    fn test_parse_empty_cargo_project() {
        let temp_dir = create_cargo_project("my-crate", &[]);
        let registry = DependencyRegistry::with_defaults();
        let manifest_path = temp_dir.path().join("Cargo.toml");

        let project = registry
            .parse_manifest(&manifest_path)
            .expect("Failed to parse Cargo.toml");

        assert_eq!(project.name, "my-crate");
        assert_eq!(project.ecosystem, Ecosystem::Cargo);
        assert!(project.dependencies.is_empty());
    }

    #[test]
    fn test_parse_cargo_with_dependencies() {
        let temp_dir = create_cargo_project(
            "my-crate",
            &[("serde", "1.0"), ("tokio", "1.0"), ("anyhow", "1.0")],
        );
        let registry = DependencyRegistry::with_defaults();
        let manifest_path = temp_dir.path().join("Cargo.toml");

        let project = registry
            .parse_manifest(&manifest_path)
            .expect("Failed to parse Cargo.toml");

        assert_eq!(project.name, "my-crate");
        assert_eq!(project.dependencies.len(), 3);

        // Check dependency names
        let dep_names: Vec<&str> = project.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(dep_names.contains(&"serde"));
        assert!(dep_names.contains(&"tokio"));
        assert!(dep_names.contains(&"anyhow"));

        // All should be Cargo ecosystem
        assert!(project.dependencies.iter().all(|d| d.ecosystem == Ecosystem::Cargo));
    }

    #[test]
    fn test_cargo_dependency_version() {
        let temp_dir = create_cargo_project("test-crate", &[("serde", "1.0.185")]);
        let registry = DependencyRegistry::with_defaults();
        let manifest_path = temp_dir.path().join("Cargo.toml");

        let project = registry
            .parse_manifest(&manifest_path)
            .expect("Failed to parse Cargo.toml");

        assert_eq!(project.dependencies.len(), 1);
        assert_eq!(project.dependencies[0].name, "serde");
        assert_eq!(project.dependencies[0].version, "1.0.185");
    }

    #[test]
    fn test_cargo_ecosystem_detection() {
        let temp_dir = create_cargo_project("test", &[]);
        let registry = DependencyRegistry::with_defaults();
        let manifest_path = temp_dir.path().join("Cargo.toml");

        let project = registry
            .parse_manifest(&manifest_path)
            .expect("Failed to parse Cargo.toml");

        assert_eq!(project.ecosystem, Ecosystem::Cargo);
    }
}

// ============================================================================
// NPM/Node.js Dependency Tests
// ============================================================================

mod npm_dependencies {
    use super::*;

    #[test]
    fn test_parse_empty_npm_project() {
        let temp_dir = create_npm_project("my-package", &[], &[]);
        let registry = DependencyRegistry::with_defaults();
        let manifest_path = temp_dir.path().join("package.json");

        let project = registry
            .parse_manifest(&manifest_path)
            .expect("Failed to parse package.json");

        assert_eq!(project.name, "my-package");
        assert_eq!(project.ecosystem, Ecosystem::Npm);
        assert!(project.dependencies.is_empty());
    }

    #[test]
    fn test_parse_npm_with_dependencies() {
        let temp_dir = create_npm_project(
            "my-package",
            &[("react", "^18.0.0"), ("lodash", "^4.17.0")],
            &[],
        );
        let registry = DependencyRegistry::with_defaults();
        let manifest_path = temp_dir.path().join("package.json");

        let project = registry
            .parse_manifest(&manifest_path)
            .expect("Failed to parse package.json");

        assert_eq!(project.name, "my-package");
        assert_eq!(project.dependencies.len(), 2);

        // Check dependency names
        let dep_names: Vec<&str> = project.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(dep_names.contains(&"react"));
        assert!(dep_names.contains(&"lodash"));

        // All should be Npm ecosystem
        assert!(project.dependencies.iter().all(|d| d.ecosystem == Ecosystem::Npm));

        // All should be non-dev
        assert!(project.dependencies.iter().all(|d| !d.is_dev));
    }

    #[test]
    fn test_parse_npm_with_dev_dependencies() {
        let temp_dir = create_npm_project(
            "my-package",
            &[("express", "^4.18.0")],
            &[("jest", "^29.0.0"), ("typescript", "^5.0.0")],
        );
        let registry = DependencyRegistry::with_defaults();
        let manifest_path = temp_dir.path().join("package.json");

        let project = registry
            .parse_manifest(&manifest_path)
            .expect("Failed to parse package.json");

        // 1 prod + 2 dev dependencies
        assert_eq!(project.dependencies.len(), 3);

        // Check dev dependencies
        let dev_deps: Vec<&str> = project
            .dependencies
            .iter()
            .filter(|d| d.is_dev)
            .map(|d| d.name.as_str())
            .collect();
        assert!(dev_deps.contains(&"jest"));
        assert!(dev_deps.contains(&"typescript"));

        // Check prod dependencies
        let prod_deps: Vec<&str> = project
            .dependencies
            .iter()
            .filter(|d| !d.is_dev)
            .map(|d| d.name.as_str())
            .collect();
        assert!(prod_deps.contains(&"express"));
    }

    #[test]
    fn test_npm_ecosystem_detection() {
        let temp_dir = create_npm_project("test", &[], &[]);
        let registry = DependencyRegistry::with_defaults();
        let manifest_path = temp_dir.path().join("package.json");

        let project = registry
            .parse_manifest(&manifest_path)
            .expect("Failed to parse package.json");

        assert_eq!(project.ecosystem, Ecosystem::Npm);
    }

    #[test]
    fn test_npm_scoped_packages() {
        let temp_dir = create_npm_project(
            "my-package",
            &[("@types/node", "^20.0.0"), ("@babel/core", "^7.0.0")],
            &[],
        );
        let registry = DependencyRegistry::with_defaults();
        let manifest_path = temp_dir.path().join("package.json");

        let project = registry
            .parse_manifest(&manifest_path)
            .expect("Failed to parse package.json");

        // Scoped packages should be parsed correctly
        let dep_names: Vec<&str> = project.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(dep_names.contains(&"@types/node"));
        assert!(dep_names.contains(&"@babel/core"));
    }
}

// ============================================================================
// Ecosystem Detection Tests
// ============================================================================

mod ecosystem_detection {
    use super::*;

    #[test]
    fn test_ecosystem_from_str() {
        assert_eq!(
            Ecosystem::from_str("cargo"),
            Some(Ecosystem::Cargo)
        );
        assert_eq!(
            Ecosystem::from_str("npm"),
            Some(Ecosystem::Npm)
        );
        assert_eq!(
            Ecosystem::from_str("rust"),
            Some(Ecosystem::Cargo)
        );
        assert_eq!(
            Ecosystem::from_str("node"),
            Some(Ecosystem::Npm)
        );
        assert_eq!(Ecosystem::from_str("unknown"), None);
    }

    #[test]
    fn test_ecosystem_as_str() {
        assert_eq!(Ecosystem::Cargo.as_str(), "cargo");
        assert_eq!(Ecosystem::Npm.as_str(), "npm");
    }
}

// ============================================================================
// Registry Tests
// ============================================================================

mod registry {
    use super::*;

    #[test]
    fn test_registry_with_defaults() {
        let registry = DependencyRegistry::with_defaults();

        // Should support Cargo and NPM
        assert!(registry.get(Ecosystem::Cargo).is_some());
        assert!(registry.get(Ecosystem::Npm).is_some());
    }

    #[test]
    fn test_registry_detect_manifest_cargo() {
        let temp_dir = create_cargo_project("test", &[]);
        let registry = DependencyRegistry::with_defaults();

        // detect_ecosystem takes a directory and looks for manifest files in it
        let ecosystem = registry.detect_ecosystem(temp_dir.path());
        assert_eq!(ecosystem, Some(Ecosystem::Cargo));
    }

    #[test]
    fn test_registry_detect_manifest_npm() {
        let temp_dir = create_npm_project("test", &[], &[]);
        let registry = DependencyRegistry::with_defaults();

        let ecosystem = registry.detect_ecosystem(temp_dir.path());
        assert_eq!(ecosystem, Some(Ecosystem::Npm));
    }

    #[test]
    fn test_registry_detect_unknown() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let registry = DependencyRegistry::with_defaults();

        // Empty directory has no manifest files
        let ecosystem = registry.detect_ecosystem(temp_dir.path());
        assert!(ecosystem.is_none());
    }
}

// ============================================================================
// Project Info Tests
// ============================================================================

mod project_info {
    use super::*;

    #[test]
    fn test_project_info_new() {
        let project = ProjectInfo::new("test-project", Ecosystem::Cargo, "/path/to/Cargo.toml");

        assert_eq!(project.name, "test-project");
        assert_eq!(project.ecosystem, Ecosystem::Cargo);
        assert_eq!(project.manifest_path, "/path/to/Cargo.toml");
        assert!(project.version.is_none());
        assert!(project.dependencies.is_empty());
    }

    #[test]
    fn test_project_info_with_version() {
        let project =
            ProjectInfo::new("test-project", Ecosystem::Npm, "/path/to/package.json")
                .with_version("1.2.3");

        assert_eq!(project.version, Some("1.2.3".to_string()));
    }

    #[test]
    fn test_project_info_from_cargo_toml() {
        let temp_dir = create_cargo_project("my-project", &[("dep1", "1.0")]);
        let registry = DependencyRegistry::with_defaults();
        let manifest_path = temp_dir.path().join("Cargo.toml");

        let project = registry.parse_manifest(&manifest_path).unwrap();

        assert_eq!(project.name, "my-project");
        assert_eq!(project.version, Some("0.1.0".to_string()));
        assert!(!project.dependencies.is_empty());
    }
}

// ============================================================================
// Dependency Struct Tests
// ============================================================================

mod dependency_struct {
    use super::*;
    use code_indexer::dependencies::Dependency;

    #[test]
    fn test_dependency_new() {
        let dep = Dependency::new("serde", "1.0.0", Ecosystem::Cargo);

        assert_eq!(dep.name, "serde");
        assert_eq!(dep.version, "1.0.0");
        assert_eq!(dep.ecosystem, Ecosystem::Cargo);
        assert!(!dep.is_dev);
        assert!(dep.source_path.is_none());
    }

    #[test]
    fn test_dependency_with_dev() {
        let dep = Dependency::new("jest", "29.0.0", Ecosystem::Npm).with_dev(true);

        assert!(dep.is_dev);
    }

    #[test]
    fn test_dependency_with_source_path() {
        let dep = Dependency::new("local-dep", "0.1.0", Ecosystem::Cargo)
            .with_source_path("/path/to/source");

        assert_eq!(dep.source_path, Some("/path/to/source".to_string()));
    }
}
