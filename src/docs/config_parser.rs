//! Configuration file parser
//!
//! Extracts scripts, build targets, and commands from package.json, Cargo.toml, Makefile.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Type of configuration file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigType {
    PackageJson,
    CargoToml,
    Makefile,
    PyProjectToml,
    GoMod,
    Other,
}

impl ConfigType {
    pub fn from_filename(filename: &str) -> Option<Self> {
        match filename.to_lowercase().as_str() {
            "package.json" => Some(ConfigType::PackageJson),
            "cargo.toml" => Some(ConfigType::CargoToml),
            "makefile" | "gnumakefile" => Some(ConfigType::Makefile),
            "pyproject.toml" => Some(ConfigType::PyProjectToml),
            "go.mod" => Some(ConfigType::GoMod),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigType::PackageJson => "package_json",
            ConfigType::CargoToml => "cargo_toml",
            ConfigType::Makefile => "makefile",
            ConfigType::PyProjectToml => "pyproject_toml",
            ConfigType::GoMod => "go_mod",
            ConfigType::Other => "other",
        }
    }
}

/// Parsed configuration digest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDigest {
    /// File path
    pub file_path: String,
    /// Configuration type
    pub config_type: ConfigType,
    /// Scripts/tasks defined (name -> command)
    pub scripts: HashMap<String, String>,
    /// Build targets
    pub build_targets: Vec<String>,
    /// Test commands
    pub test_commands: Vec<String>,
    /// Run/start commands
    pub run_commands: Vec<String>,
    /// Project name (if found)
    pub name: Option<String>,
    /// Project version (if found)
    pub version: Option<String>,
}

/// Parser for configuration files
pub struct ConfigParser;

impl ConfigParser {
    /// Parse a configuration file
    pub fn parse(file_path: &str, content: &str) -> Option<ConfigDigest> {
        let filename = std::path::Path::new(file_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let config_type = ConfigType::from_filename(&filename)?;

        let digest = match config_type {
            ConfigType::PackageJson => Self::parse_package_json(file_path, content),
            ConfigType::CargoToml => Self::parse_cargo_toml(file_path, content),
            ConfigType::Makefile => Self::parse_makefile(file_path, content),
            ConfigType::PyProjectToml => Self::parse_pyproject_toml(file_path, content),
            ConfigType::GoMod => Self::parse_go_mod(file_path, content),
            ConfigType::Other => None,
        };

        digest
    }

    fn parse_package_json(file_path: &str, content: &str) -> Option<ConfigDigest> {
        let json: serde_json::Value = serde_json::from_str(content).ok()?;

        let mut scripts = HashMap::new();
        let mut build_targets = Vec::new();
        let mut test_commands = Vec::new();
        let mut run_commands = Vec::new();

        // Extract scripts
        if let Some(scripts_obj) = json.get("scripts").and_then(|s| s.as_object()) {
            for (name, cmd) in scripts_obj {
                if let Some(cmd_str) = cmd.as_str() {
                    scripts.insert(name.clone(), cmd_str.to_string());

                    // Categorize
                    let lower_name = name.to_lowercase();
                    if lower_name.contains("build") || lower_name.contains("compile") {
                        build_targets.push(name.clone());
                    }
                    if lower_name.contains("test") || lower_name == "jest" || lower_name == "mocha" {
                        test_commands.push(cmd_str.to_string());
                    }
                    if lower_name == "start" || lower_name == "dev" || lower_name == "serve" {
                        run_commands.push(cmd_str.to_string());
                    }
                }
            }
        }

        // Check main/bin for entry points
        if let Some(main) = json.get("main").and_then(|m| m.as_str()) {
            run_commands.push(format!("node {}", main));
        }
        if let Some(bin) = json.get("bin") {
            if let Some(bin_obj) = bin.as_object() {
                for (name, _) in bin_obj {
                    run_commands.push(format!("npx {}", name));
                }
            } else if let Some(bin_str) = bin.as_str() {
                run_commands.push(format!("node {}", bin_str));
            }
        }

        let name = json.get("name").and_then(|n| n.as_str()).map(String::from);
        let version = json.get("version").and_then(|v| v.as_str()).map(String::from);

        Some(ConfigDigest {
            file_path: file_path.to_string(),
            config_type: ConfigType::PackageJson,
            scripts,
            build_targets,
            test_commands,
            run_commands,
            name,
            version,
        })
    }

    fn parse_cargo_toml(file_path: &str, content: &str) -> Option<ConfigDigest> {
        let toml: toml::Value = content.parse().ok()?;

        let mut scripts = HashMap::new();
        let mut build_targets = Vec::new();
        let mut test_commands = Vec::new();
        let mut run_commands = Vec::new();

        // Standard cargo commands
        scripts.insert("build".to_string(), "cargo build".to_string());
        scripts.insert("test".to_string(), "cargo test".to_string());
        scripts.insert("run".to_string(), "cargo run".to_string());
        scripts.insert("check".to_string(), "cargo check".to_string());

        build_targets.push("build".to_string());
        test_commands.push("cargo test".to_string());
        run_commands.push("cargo run".to_string());

        // Check for [[bin]] targets
        if let Some(bins) = toml.get("bin").and_then(|b| b.as_array()) {
            for bin in bins {
                if let Some(name) = bin.get("name").and_then(|n| n.as_str()) {
                    run_commands.push(format!("cargo run --bin {}", name));
                }
            }
        }

        // Check for examples
        if let Some(examples) = toml.get("example").and_then(|e| e.as_array()) {
            for ex in examples {
                if let Some(name) = ex.get("name").and_then(|n| n.as_str()) {
                    run_commands.push(format!("cargo run --example {}", name));
                }
            }
        }

        // Check for workspace
        if toml.get("workspace").is_some() {
            build_targets.push("workspace".to_string());
        }

        let name = toml.get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(String::from);

        let version = toml.get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .map(String::from);

        Some(ConfigDigest {
            file_path: file_path.to_string(),
            config_type: ConfigType::CargoToml,
            scripts,
            build_targets,
            test_commands,
            run_commands,
            name,
            version,
        })
    }

    fn parse_makefile(file_path: &str, content: &str) -> Option<ConfigDigest> {
        let mut scripts = HashMap::new();
        let mut build_targets = Vec::new();
        let mut test_commands = Vec::new();
        let mut run_commands = Vec::new();

        // Parse Makefile targets
        for line in content.lines() {
            // Match target: dependencies pattern
            if let Some(colon_pos) = line.find(':') {
                let target = line[..colon_pos].trim();

                // Skip special targets and pattern rules
                if target.starts_with('.') || target.starts_with('%') || target.contains('$') {
                    continue;
                }

                // Skip empty or multi-line targets
                if target.is_empty() || target.contains(' ') {
                    continue;
                }

                scripts.insert(target.to_string(), format!("make {}", target));

                let lower_target = target.to_lowercase();
                if lower_target.contains("build") || lower_target == "all" || lower_target == "compile" {
                    build_targets.push(target.to_string());
                }
                if lower_target.contains("test") || lower_target == "check" {
                    test_commands.push(format!("make {}", target));
                }
                if lower_target == "run" || lower_target == "start" || lower_target == "serve" {
                    run_commands.push(format!("make {}", target));
                }
            }
        }

        Some(ConfigDigest {
            file_path: file_path.to_string(),
            config_type: ConfigType::Makefile,
            scripts,
            build_targets,
            test_commands,
            run_commands,
            name: None,
            version: None,
        })
    }

    fn parse_pyproject_toml(file_path: &str, content: &str) -> Option<ConfigDigest> {
        let toml: toml::Value = content.parse().ok()?;

        let mut scripts = HashMap::new();
        let mut build_targets = Vec::new();
        let mut test_commands = Vec::new();
        let mut run_commands = Vec::new();

        // Poetry scripts
        if let Some(poetry_scripts) = toml.get("tool")
            .and_then(|t| t.get("poetry"))
            .and_then(|p| p.get("scripts"))
            .and_then(|s| s.as_table())
        {
            for (name, cmd) in poetry_scripts {
                if let Some(cmd_str) = cmd.as_str() {
                    scripts.insert(name.clone(), cmd_str.to_string());
                    run_commands.push(format!("poetry run {}", name));
                }
            }
        }

        // Check for pytest
        if toml.get("tool")
            .and_then(|t| t.get("pytest"))
            .is_some()
        {
            test_commands.push("pytest".to_string());
            scripts.insert("test".to_string(), "pytest".to_string());
        }

        // Standard commands
        scripts.insert("build".to_string(), "python -m build".to_string());
        build_targets.push("build".to_string());

        let name = toml.get("project")
            .or_else(|| toml.get("tool").and_then(|t| t.get("poetry")))
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(String::from);

        let version = toml.get("project")
            .or_else(|| toml.get("tool").and_then(|t| t.get("poetry")))
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .map(String::from);

        Some(ConfigDigest {
            file_path: file_path.to_string(),
            config_type: ConfigType::PyProjectToml,
            scripts,
            build_targets,
            test_commands,
            run_commands,
            name,
            version,
        })
    }

    fn parse_go_mod(file_path: &str, content: &str) -> Option<ConfigDigest> {
        let mut scripts = HashMap::new();
        let mut build_targets = Vec::new();
        let mut test_commands = Vec::new();
        let mut run_commands = Vec::new();
        let mut name = None;

        // Parse module name
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("module ") {
                name = Some(trimmed[7..].trim().to_string());
                break;
            }
        }

        // Standard Go commands
        scripts.insert("build".to_string(), "go build".to_string());
        scripts.insert("test".to_string(), "go test ./...".to_string());
        scripts.insert("run".to_string(), "go run .".to_string());

        build_targets.push("build".to_string());
        test_commands.push("go test ./...".to_string());
        run_commands.push("go run .".to_string());

        Some(ConfigDigest {
            file_path: file_path.to_string(),
            config_type: ConfigType::GoMod,
            scripts,
            build_targets,
            test_commands,
            run_commands,
            name,
            version: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_package_json() {
        let content = r#"{
            "name": "my-project",
            "version": "1.0.0",
            "scripts": {
                "build": "tsc",
                "test": "jest",
                "start": "node dist/index.js",
                "dev": "nodemon"
            },
            "main": "dist/index.js"
        }"#;

        let digest = ConfigParser::parse("package.json", content).unwrap();

        assert_eq!(digest.config_type, ConfigType::PackageJson);
        assert_eq!(digest.name.as_deref(), Some("my-project"));
        assert_eq!(digest.version.as_deref(), Some("1.0.0"));
        assert_eq!(digest.scripts.get("build"), Some(&"tsc".to_string()));
        assert!(digest.build_targets.contains(&"build".to_string()));
        assert!(digest.test_commands.contains(&"jest".to_string()));
    }

    #[test]
    fn test_parse_cargo_toml() {
        let content = r#"
[package]
name = "my-crate"
version = "0.1.0"

[[bin]]
name = "mycli"
"#;

        let digest = ConfigParser::parse("Cargo.toml", content).unwrap();

        assert_eq!(digest.config_type, ConfigType::CargoToml);
        assert_eq!(digest.name.as_deref(), Some("my-crate"));
        assert!(digest.run_commands.contains(&"cargo run --bin mycli".to_string()));
    }

    #[test]
    fn test_parse_makefile() {
        let content = r#"
.PHONY: all build test clean

all: build

build:
	go build -o app

test:
	go test ./...

run:
	./app
"#;

        let digest = ConfigParser::parse("Makefile", content).unwrap();

        assert_eq!(digest.config_type, ConfigType::Makefile);
        assert!(digest.build_targets.contains(&"build".to_string()));
        assert!(digest.build_targets.contains(&"all".to_string()));
        assert!(digest.test_commands.contains(&"make test".to_string()));
    }

    #[test]
    fn test_config_type_from_filename() {
        assert_eq!(ConfigType::from_filename("package.json"), Some(ConfigType::PackageJson));
        assert_eq!(ConfigType::from_filename("Cargo.toml"), Some(ConfigType::CargoToml));
        assert_eq!(ConfigType::from_filename("MAKEFILE"), Some(ConfigType::Makefile));
        assert_eq!(ConfigType::from_filename("random.txt"), None);
    }
}
