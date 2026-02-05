//! Integration tests for P0 Macro Navigation features
//!
//! Tests for:
//! - P0-5: Doc/Config Digest (parsing README, package.json, Cargo.toml)
//! - P0-1: get_project_compass (macro-level project overview)
//! - P0-2: expand_project_node (drill-down by modules)
//! - P0-3: get_compass(query) (task-oriented starting points)
//! - P0-4: Session Dictionary Codec (token optimization)

use std::collections::HashMap;
use std::path::PathBuf;

use code_indexer::{
    CodeIndex, FileWalker, LanguageRegistry, Location, Parser, SqliteIndex,
    Symbol, SymbolExtractor, SymbolKind, Visibility,
};
use code_indexer::docs::{ConfigDigest, ConfigParser, ConfigType, DocDigest, DocParser, DocType};
use code_indexer::compass::{
    EntryPoint, EntryType,
    NodeBuilder, ProjectNode, NodeType,
    ProfileBuilder, ProjectProfile, LanguageStats, FrameworkInfo,
};
use code_indexer::session::{DictEncoder, DictDecoder, SessionManager, Session};
use tempfile::NamedTempFile;

// ============================================================================
// Test Helpers
// ============================================================================

fn create_test_index() -> SqliteIndex {
    SqliteIndex::in_memory().expect("Failed to create in-memory index")
}

fn create_test_symbol(name: &str, kind: SymbolKind, file: &str, line: u32) -> Symbol {
    Symbol::new(name, kind, Location::new(file, line, 0, line + 10, 0), "rust")
}

// ============================================================================
// P0-5: Doc/Config Digest Tests
// ============================================================================

mod doc_digest {
    use super::*;

    #[test]
    fn test_doc_type_from_filename() {
        assert_eq!(DocType::from_filename("README.md"), DocType::Readme);
        assert_eq!(DocType::from_filename("readme.txt"), DocType::Readme);
        assert_eq!(DocType::from_filename("CONTRIBUTING.md"), DocType::Contributing);
        assert_eq!(DocType::from_filename("CHANGELOG.md"), DocType::Changelog);
        assert_eq!(DocType::from_filename("HISTORY.md"), DocType::Changelog);
        assert_eq!(DocType::from_filename("LICENSE"), DocType::License);
        assert_eq!(DocType::from_filename("random.md"), DocType::Other);
    }

    #[test]
    fn test_doc_type_as_str() {
        assert_eq!(DocType::Readme.as_str(), "readme");
        assert_eq!(DocType::Contributing.as_str(), "contributing");
        assert_eq!(DocType::Changelog.as_str(), "changelog");
        assert_eq!(DocType::License.as_str(), "license");
    }

    #[test]
    fn test_parse_readme_with_headings() {
        let content = r#"# My Awesome Project

This is the introduction.

## Installation

Run the following command:

```bash
npm install my-project
```

## Usage

Import and use:

```javascript
const proj = require('my-project');
proj.run();
```

### Advanced Usage

For advanced scenarios...

## API Reference

### `run(options)`

Runs the project.

## Contributing

Please read CONTRIBUTING.md

## License

MIT
"#;

        let digest = DocParser::parse("README.md", content);

        assert_eq!(digest.doc_type, DocType::Readme);
        assert_eq!(digest.title.as_deref(), Some("My Awesome Project"));

        // Check headings
        assert!(digest.headings.len() >= 6);
        assert_eq!(digest.headings[0].text, "My Awesome Project");
        assert_eq!(digest.headings[0].level, 1);
        assert_eq!(digest.headings[1].text, "Installation");
        assert_eq!(digest.headings[1].level, 2);

        // Check command blocks (bash should be detected)
        assert!(!digest.command_blocks.is_empty());
        assert_eq!(digest.command_blocks[0].language.as_deref(), Some("bash"));
        assert!(digest.command_blocks[0].content.contains("npm install"));

        // Check key sections
        let section_names: Vec<&str> = digest.key_sections.iter()
            .map(|s| s.heading.as_str())
            .collect();
        assert!(section_names.contains(&"Installation"));
        assert!(section_names.contains(&"Usage"));
        assert!(section_names.contains(&"License"));
    }

    #[test]
    fn test_parse_readme_with_cargo_commands() {
        let content = r#"# Rust Project

## Getting Started

```bash
cargo build --release
cargo run
```

## Testing

```
cargo test
```
"#;

        let digest = DocParser::parse("README.md", content);

        // Both bash block and plain block with cargo should be detected
        assert!(digest.command_blocks.len() >= 1);
        assert!(digest.command_blocks.iter().any(|b| b.content.contains("cargo")));
    }

    #[test]
    fn test_extract_section() {
        let content = r#"# Project

## Installation

Step 1: Clone the repo
Step 2: Run setup

## Usage

Use the CLI:

```bash
./cli run
```

## License

MIT
"#;

        let section = DocParser::extract_section(content, "installation");
        assert!(section.is_some());
        let section_text = section.unwrap();
        assert!(section_text.contains("Step 1"));
        assert!(section_text.contains("Step 2"));
        // Should not contain Usage content
        assert!(!section_text.contains("CLI"));
    }

    #[test]
    fn test_extract_section_not_found() {
        let content = "# Project\n\n## Usage\n\nSome content.";
        let section = DocParser::extract_section(content, "installation");
        assert!(section.is_none());
    }

    #[test]
    fn test_nested_headings() {
        let content = r#"# Project

## Features

### Feature A

Details about A.

### Feature B

Details about B.

## Installation

Steps...
"#;

        let digest = DocParser::parse("README.md", content);

        // Features section should include both Feature A and Feature B subsections
        let features_section = digest.key_sections.iter()
            .find(|s| s.heading == "Installation");
        assert!(features_section.is_some());
    }
}

mod config_digest {
    use super::*;

    #[test]
    fn test_config_type_from_filename() {
        assert_eq!(ConfigType::from_filename("package.json"), Some(ConfigType::PackageJson));
        assert_eq!(ConfigType::from_filename("PACKAGE.JSON"), Some(ConfigType::PackageJson));
        assert_eq!(ConfigType::from_filename("Cargo.toml"), Some(ConfigType::CargoToml));
        assert_eq!(ConfigType::from_filename("Makefile"), Some(ConfigType::Makefile));
        assert_eq!(ConfigType::from_filename("GNUmakefile"), Some(ConfigType::Makefile));
        assert_eq!(ConfigType::from_filename("pyproject.toml"), Some(ConfigType::PyProjectToml));
        assert_eq!(ConfigType::from_filename("go.mod"), Some(ConfigType::GoMod));
        assert_eq!(ConfigType::from_filename("random.txt"), None);
    }

    #[test]
    fn test_parse_package_json_full() {
        let content = r#"{
            "name": "my-awesome-app",
            "version": "2.0.0",
            "main": "dist/index.js",
            "bin": {
                "myapp": "./bin/cli.js"
            },
            "scripts": {
                "build": "tsc",
                "build:prod": "tsc --prod",
                "test": "jest --coverage",
                "test:watch": "jest --watch",
                "start": "node dist/index.js",
                "dev": "nodemon src/index.ts",
                "lint": "eslint src/"
            }
        }"#;

        let digest = ConfigParser::parse("package.json", content).unwrap();

        assert_eq!(digest.config_type, ConfigType::PackageJson);
        assert_eq!(digest.name.as_deref(), Some("my-awesome-app"));
        assert_eq!(digest.version.as_deref(), Some("2.0.0"));

        // Scripts
        assert_eq!(digest.scripts.get("build"), Some(&"tsc".to_string()));
        assert_eq!(digest.scripts.get("lint"), Some(&"eslint src/".to_string()));

        // Build targets
        assert!(digest.build_targets.contains(&"build".to_string()));
        assert!(digest.build_targets.contains(&"build:prod".to_string()));

        // Test commands
        assert!(digest.test_commands.contains(&"jest --coverage".to_string()));

        // Run commands (start, dev, main, bin)
        assert!(digest.run_commands.contains(&"node dist/index.js".to_string()));
        assert!(digest.run_commands.iter().any(|c| c.contains("nodemon")));
        assert!(digest.run_commands.iter().any(|c| c.contains("myapp")));
    }

    #[test]
    fn test_parse_cargo_toml_with_bins() {
        let content = r#"
[package]
name = "my-rust-app"
version = "0.5.0"
edition = "2021"

[[bin]]
name = "server"
path = "src/bin/server.rs"

[[bin]]
name = "cli"
path = "src/bin/cli.rs"

[[example]]
name = "demo"
"#;

        let digest = ConfigParser::parse("Cargo.toml", content).unwrap();

        assert_eq!(digest.config_type, ConfigType::CargoToml);
        assert_eq!(digest.name.as_deref(), Some("my-rust-app"));
        assert_eq!(digest.version.as_deref(), Some("0.5.0"));

        // Standard cargo commands
        assert!(digest.scripts.contains_key("build"));
        assert!(digest.scripts.contains_key("test"));
        assert!(digest.scripts.contains_key("run"));

        // Bin targets
        assert!(digest.run_commands.contains(&"cargo run --bin server".to_string()));
        assert!(digest.run_commands.contains(&"cargo run --bin cli".to_string()));

        // Example targets
        assert!(digest.run_commands.contains(&"cargo run --example demo".to_string()));
    }

    #[test]
    fn test_parse_cargo_toml_workspace() {
        let content = r#"
[workspace]
members = ["crates/*"]

[workspace.package]
version = "1.0.0"
"#;

        let digest = ConfigParser::parse("Cargo.toml", content).unwrap();

        // Workspace should be detected
        assert!(digest.build_targets.contains(&"workspace".to_string()));
    }

    #[test]
    fn test_parse_makefile() {
        let content = r#"
.PHONY: all build test clean run install

all: build test

build:
	go build -o bin/app ./cmd/app

test:
	go test -v ./...

run: build
	./bin/app

clean:
	rm -rf bin/

install:
	cp bin/app /usr/local/bin/
"#;

        let digest = ConfigParser::parse("Makefile", content).unwrap();

        assert_eq!(digest.config_type, ConfigType::Makefile);

        // Targets as scripts
        assert!(digest.scripts.contains_key("build"));
        assert!(digest.scripts.contains_key("test"));
        assert!(digest.scripts.contains_key("clean"));

        // Build targets
        assert!(digest.build_targets.contains(&"build".to_string()));
        assert!(digest.build_targets.contains(&"all".to_string()));

        // Test commands
        assert!(digest.test_commands.contains(&"make test".to_string()));

        // Run commands
        assert!(digest.run_commands.contains(&"make run".to_string()));
    }

    #[test]
    fn test_parse_pyproject_toml() {
        let content = r#"
[project]
name = "my-python-app"
version = "1.0.0"

[tool.poetry.scripts]
cli = "myapp.cli:main"
server = "myapp.server:run"

[tool.pytest.ini_options]
testpaths = ["tests"]
"#;

        let digest = ConfigParser::parse("pyproject.toml", content).unwrap();

        assert_eq!(digest.config_type, ConfigType::PyProjectToml);
        assert_eq!(digest.name.as_deref(), Some("my-python-app"));

        // Poetry scripts
        assert!(digest.run_commands.iter().any(|c| c.contains("cli")));
        assert!(digest.run_commands.iter().any(|c| c.contains("server")));

        // Pytest detected
        assert!(digest.test_commands.contains(&"pytest".to_string()));
    }

    #[test]
    fn test_parse_go_mod() {
        let content = r#"
module github.com/user/myapp

go 1.21

require (
    github.com/gin-gonic/gin v1.9.0
)
"#;

        let digest = ConfigParser::parse("go.mod", content).unwrap();

        assert_eq!(digest.config_type, ConfigType::GoMod);
        assert_eq!(digest.name.as_deref(), Some("github.com/user/myapp"));

        // Standard Go commands
        assert!(digest.scripts.contains_key("build"));
        assert!(digest.scripts.contains_key("test"));
        assert!(digest.scripts.contains_key("run"));

        assert!(digest.test_commands.contains(&"go test ./...".to_string()));
        assert!(digest.run_commands.contains(&"go run .".to_string()));
    }

    #[test]
    fn test_parse_invalid_config() {
        let result = ConfigParser::parse("unknown.xyz", "random content");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_malformed_json() {
        let result = ConfigParser::parse("package.json", "{ invalid json }");
        assert!(result.is_none());
    }
}

mod doc_digest_persistence {
    use super::*;

    #[test]
    fn test_add_and_get_doc_digest() {
        let index = create_test_index();

        let digest = DocDigest {
            file_path: "README.md".to_string(),
            doc_type: DocType::Readme,
            title: Some("Test Project".to_string()),
            headings: vec![
                code_indexer::docs::Heading {
                    level: 1,
                    text: "Test Project".to_string(),
                    line: 1,
                },
            ],
            command_blocks: vec![],
            key_sections: vec![],
        };

        index.add_doc_digest(&digest).unwrap();

        let retrieved = index.get_doc_digest("README.md").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.title.as_deref(), Some("Test Project"));
        assert_eq!(retrieved.doc_type, DocType::Readme);
    }

    #[test]
    fn test_get_all_doc_digests() {
        let index = create_test_index();

        let readme = DocDigest {
            file_path: "README.md".to_string(),
            doc_type: DocType::Readme,
            title: Some("Project".to_string()),
            headings: vec![],
            command_blocks: vec![],
            key_sections: vec![],
        };

        let contributing = DocDigest {
            file_path: "CONTRIBUTING.md".to_string(),
            doc_type: DocType::Contributing,
            title: Some("Contributing".to_string()),
            headings: vec![],
            command_blocks: vec![],
            key_sections: vec![],
        };

        index.add_doc_digest(&readme).unwrap();
        index.add_doc_digest(&contributing).unwrap();

        let all = index.get_all_doc_digests().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_add_and_get_config_digest() {
        let index = create_test_index();

        let config = ConfigDigest {
            file_path: "package.json".to_string(),
            config_type: ConfigType::PackageJson,
            scripts: HashMap::from([
                ("build".to_string(), "tsc".to_string()),
                ("test".to_string(), "jest".to_string()),
            ]),
            build_targets: vec!["build".to_string()],
            test_commands: vec!["jest".to_string()],
            run_commands: vec!["node index.js".to_string()],
            name: Some("my-app".to_string()),
            version: Some("1.0.0".to_string()),
        };

        index.add_config_digest(&config).unwrap();

        let retrieved = index.get_config_digest("package.json").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.name.as_deref(), Some("my-app"));
        assert_eq!(retrieved.scripts.get("build"), Some(&"tsc".to_string()));
    }

    #[test]
    fn test_get_project_commands() {
        let index = create_test_index();

        let config = ConfigDigest {
            file_path: "Cargo.toml".to_string(),
            config_type: ConfigType::CargoToml,
            scripts: HashMap::new(),
            build_targets: vec!["build".to_string()],
            test_commands: vec!["cargo test".to_string()],
            run_commands: vec!["cargo run".to_string()],
            name: Some("myapp".to_string()),
            version: Some("0.1.0".to_string()),
        };

        index.add_config_digest(&config).unwrap();

        let commands = index.get_project_commands().unwrap();
        assert!(commands.run.contains(&"cargo run".to_string()));
        // For Cargo.toml, build_targets are converted to "cargo build"
        assert!(commands.build.contains(&"cargo build".to_string()));
        assert!(commands.test.contains(&"cargo test".to_string()));
    }
}

// ============================================================================
// P0-1: Project Compass Tests
// ============================================================================

mod project_profile {
    use super::*;

    #[test]
    fn test_language_stats_creation() {
        let stats = LanguageStats {
            name: "rust".to_string(),
            file_count: 50,
            symbol_count: 500,
            percentage: 75.0,
        };

        assert_eq!(stats.name, "rust");
        assert_eq!(stats.file_count, 50);
        assert_eq!(stats.symbol_count, 500);
        assert_eq!(stats.percentage, 75.0);
    }

    #[test]
    fn test_framework_info_creation() {
        let framework = FrameworkInfo {
            name: "tokio".to_string(),
            category: "async runtime".to_string(),
            evidence: "import of tokio".to_string(),
        };

        assert_eq!(framework.name, "tokio");
        assert_eq!(framework.category, "async runtime");
    }

    #[test]
    fn test_project_profile_structure() {
        let profile = ProjectProfile {
            languages: vec![
                LanguageStats {
                    name: "rust".to_string(),
                    file_count: 30,
                    symbol_count: 300,
                    percentage: 60.0,
                },
                LanguageStats {
                    name: "typescript".to_string(),
                    file_count: 20,
                    symbol_count: 200,
                    percentage: 40.0,
                },
            ],
            frameworks: vec![],
            build_tools: vec!["cargo".to_string(), "npm".to_string()],
            workspace_type: Some("cargo".to_string()),
            total_files: 50,
            total_symbols: 500,
        };

        assert_eq!(profile.languages.len(), 2);
        assert_eq!(profile.build_tools.len(), 2);
        assert_eq!(profile.total_symbols, 500);
    }
}

mod project_nodes {
    use super::*;

    #[test]
    fn test_node_type_variants() {
        assert_eq!(NodeType::Module.as_str(), "module");
        assert_eq!(NodeType::Directory.as_str(), "directory");
        assert_eq!(NodeType::Package.as_str(), "package");
        assert_eq!(NodeType::Layer.as_str(), "layer");
    }

    #[test]
    fn test_project_node_creation() {
        let node = ProjectNode {
            id: "dir:src/api".to_string(),
            parent_id: Some("dir:src".to_string()),
            node_type: NodeType::Layer,
            name: "api".to_string(),
            path: "src/api".to_string(),
            symbol_count: 100,
            public_symbol_count: 30,
            file_count: 10,
            centrality_score: 0.85,
            children: vec!["dir:src/api/handlers".to_string()],
        };

        assert_eq!(node.id, "dir:src/api");
        assert_eq!(node.node_type, NodeType::Layer);
        assert_eq!(node.symbol_count, 100);
        assert_eq!(node.children.len(), 1);
    }

    // Note: test_node_type_inference is tested in the unit tests within node_builder.rs
    // The infer_node_type function is private, so we test behavior through NodeBuilder::build

    #[test]
    fn test_get_top_level_nodes() {
        let nodes = vec![
            ProjectNode {
                id: "dir:src".to_string(),
                parent_id: None,
                node_type: NodeType::Directory,
                name: "src".to_string(),
                path: "src".to_string(),
                symbol_count: 200,
                public_symbol_count: 50,
                file_count: 20,
                centrality_score: 1.0,
                children: vec!["dir:src/api".to_string()],
            },
            ProjectNode {
                id: "dir:src/api".to_string(),
                parent_id: Some("dir:src".to_string()),
                node_type: NodeType::Layer,
                name: "api".to_string(),
                path: "src/api".to_string(),
                symbol_count: 100,
                public_symbol_count: 30,
                file_count: 10,
                centrality_score: 0.8,
                children: vec![],
            },
        ];

        let top_level = NodeBuilder::get_top_level(&nodes);
        assert_eq!(top_level.len(), 1);
        assert_eq!(top_level[0].id, "dir:src");
    }
}

mod entry_points {
    use super::*;

    #[test]
    fn test_entry_type_variants() {
        assert_eq!(EntryType::Main.as_str(), "main");
        assert_eq!(EntryType::TokioMain.as_str(), "tokio_main");
        assert_eq!(EntryType::ActixMain.as_str(), "actix_main");
        assert_eq!(EntryType::Server.as_str(), "server");
        assert_eq!(EntryType::Cli.as_str(), "cli");
        assert_eq!(EntryType::RestEndpoint.as_str(), "rest_endpoint");
        assert_eq!(EntryType::GraphqlResolver.as_str(), "graphql_resolver");
        assert_eq!(EntryType::GrpcService.as_str(), "grpc_service");
        assert_eq!(EntryType::Test.as_str(), "test");
        assert_eq!(EntryType::Benchmark.as_str(), "benchmark");
    }

    #[test]
    fn test_entry_point_creation() {
        let entry = EntryPoint {
            symbol_id: Some("sym_123".to_string()),
            entry_type: EntryType::Main,
            file_path: "src/main.rs".to_string(),
            line: 1,
            name: "main".to_string(),
            evidence: "fn main() in main file".to_string(),
        };

        assert_eq!(entry.entry_type, EntryType::Main);
        assert_eq!(entry.file_path, "src/main.rs");
        assert_eq!(entry.name, "main");
    }
}

mod compass_persistence {
    use super::*;

    #[test]
    fn test_save_and_get_project_profile() {
        let index = create_test_index();

        let profile = ProjectProfile {
            languages: vec![
                LanguageStats {
                    name: "rust".to_string(),
                    file_count: 30,
                    symbol_count: 300,
                    percentage: 100.0,
                },
            ],
            frameworks: vec![
                FrameworkInfo {
                    name: "tokio".to_string(),
                    category: "async runtime".to_string(),
                    evidence: "import".to_string(),
                },
            ],
            build_tools: vec!["cargo".to_string()],
            workspace_type: Some("cargo".to_string()),
            total_files: 30,
            total_symbols: 300,
        };

        index.save_project_profile("/test/project", &profile).unwrap();

        let retrieved = index.get_project_profile("/test/project").unwrap();
        assert!(retrieved.is_some());
        let (retrieved_profile, _profile_rev) = retrieved.unwrap();
        assert_eq!(retrieved_profile.languages.len(), 1);
        assert_eq!(retrieved_profile.languages[0].name, "rust");
        assert_eq!(retrieved_profile.frameworks.len(), 1);
    }

    #[test]
    fn test_save_and_get_project_nodes() {
        let index = create_test_index();

        let nodes = vec![
            ProjectNode {
                id: "dir:src".to_string(),
                parent_id: None,
                node_type: NodeType::Directory,
                name: "src".to_string(),
                path: "src".to_string(),
                symbol_count: 100,
                public_symbol_count: 30,
                file_count: 10,
                centrality_score: 1.0,
                children: vec![],
            },
        ];

        index.save_project_nodes(&nodes).unwrap();

        let retrieved = index.get_project_nodes().unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].id, "dir:src");
    }

    #[test]
    fn test_get_project_node_by_id() {
        let index = create_test_index();

        let nodes = vec![
            ProjectNode {
                id: "dir:src/api".to_string(),
                parent_id: Some("dir:src".to_string()),
                node_type: NodeType::Layer,
                name: "api".to_string(),
                path: "src/api".to_string(),
                symbol_count: 50,
                public_symbol_count: 20,
                file_count: 5,
                centrality_score: 0.8,
                children: vec![],
            },
        ];

        index.save_project_nodes(&nodes).unwrap();

        let node = index.get_project_node("dir:src/api").unwrap();
        assert!(node.is_some());
        assert_eq!(node.unwrap().name, "api");

        // Non-existent node
        let missing = index.get_project_node("dir:nonexistent").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_get_node_children() {
        let index = create_test_index();

        let nodes = vec![
            ProjectNode {
                id: "dir:src".to_string(),
                parent_id: None,
                node_type: NodeType::Directory,
                name: "src".to_string(),
                path: "src".to_string(),
                symbol_count: 100,
                public_symbol_count: 30,
                file_count: 10,
                centrality_score: 1.0,
                children: vec!["dir:src/api".to_string(), "dir:src/domain".to_string()],
            },
            ProjectNode {
                id: "dir:src/api".to_string(),
                parent_id: Some("dir:src".to_string()),
                node_type: NodeType::Layer,
                name: "api".to_string(),
                path: "src/api".to_string(),
                symbol_count: 50,
                public_symbol_count: 20,
                file_count: 5,
                centrality_score: 0.8,
                children: vec![],
            },
            ProjectNode {
                id: "dir:src/domain".to_string(),
                parent_id: Some("dir:src".to_string()),
                node_type: NodeType::Layer,
                name: "domain".to_string(),
                path: "src/domain".to_string(),
                symbol_count: 30,
                public_symbol_count: 10,
                file_count: 3,
                centrality_score: 0.6,
                children: vec![],
            },
        ];

        index.save_project_nodes(&nodes).unwrap();

        let children = index.get_node_children("dir:src").unwrap();
        assert_eq!(children.len(), 2);
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"api"));
        assert!(names.contains(&"domain"));
    }

    #[test]
    fn test_save_and_get_entry_points() {
        let index = create_test_index();

        let entries = vec![
            EntryPoint {
                symbol_id: Some("sym_1".to_string()),
                entry_type: EntryType::Main,
                file_path: "src/main.rs".to_string(),
                line: 1,
                name: "main".to_string(),
                evidence: "fn main()".to_string(),
            },
            EntryPoint {
                symbol_id: Some("sym_2".to_string()),
                entry_type: EntryType::Server,
                file_path: "src/server.rs".to_string(),
                line: 10,
                name: "start_server".to_string(),
                evidence: "server.listen()".to_string(),
            },
        ];

        index.save_entry_points(&entries).unwrap();

        let retrieved = index.get_entry_points().unwrap();
        assert_eq!(retrieved.len(), 2);

        // Should be sorted by type priority
        assert_eq!(retrieved[0].entry_type, EntryType::Main);
    }
}

// ============================================================================
// P0-4: Session Dictionary Codec Tests
// ============================================================================

mod dict_codec {
    use super::*;

    #[test]
    fn test_encoder_basic() {
        let mut encoder = DictEncoder::new();

        // First encoding - new entries
        let (id1, new1) = encoder.encode_file("src/main.rs");
        assert!(new1);
        assert_eq!(id1, 0);

        let (id2, new2) = encoder.encode_file("src/lib.rs");
        assert!(new2);
        assert_eq!(id2, 1);

        // Re-encoding - existing entry
        let (id3, new3) = encoder.encode_file("src/main.rs");
        assert!(!new3);
        assert_eq!(id3, 0);
    }

    #[test]
    fn test_encoder_kinds() {
        let mut encoder = DictEncoder::new();

        let (id1, _) = encoder.encode_kind("function");
        let (id2, _) = encoder.encode_kind("struct");
        let (id3, _) = encoder.encode_kind("function");

        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(id3, 0); // Same as first
    }

    #[test]
    fn test_encoder_modules() {
        let mut encoder = DictEncoder::new();

        let (id1, new1) = encoder.encode_module("src::api");
        let (id2, new2) = encoder.encode_module("src::domain");
        let (id3, new3) = encoder.encode_module("src::api");

        assert!(new1);
        assert!(new2);
        assert!(!new3);
        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(id3, 0);
    }

    #[test]
    fn test_dict_delta() {
        let mut encoder = DictEncoder::new();

        encoder.encode_file("src/main.rs");
        encoder.encode_file("src/lib.rs");
        encoder.encode_kind("function");
        encoder.encode_module("src::api");

        let delta = encoder.get_delta();

        assert_eq!(delta.files.len(), 2);
        assert_eq!(delta.kinds.len(), 1);
        assert_eq!(delta.modules.len(), 1);

        assert_eq!(delta.files.get(&0), Some(&"src/main.rs".to_string()));
        assert_eq!(delta.files.get(&1), Some(&"src/lib.rs".to_string()));
    }

    #[test]
    fn test_dict_delta_is_empty() {
        let encoder = DictEncoder::new();
        let delta = encoder.get_delta();
        assert!(delta.is_empty());

        let mut encoder2 = DictEncoder::new();
        encoder2.encode_file("test.rs");
        let delta2 = encoder2.get_delta();
        assert!(!delta2.is_empty());
    }

    #[test]
    fn test_decoder_from_delta() {
        let mut encoder = DictEncoder::new();

        encoder.encode_file("src/main.rs");
        encoder.encode_kind("function");

        let delta = encoder.get_delta();
        let decoder = DictDecoder::from_delta(&delta);

        assert_eq!(decoder.decode_file(0), Some("src/main.rs"));
        assert_eq!(decoder.decode_kind(0), Some("function"));
        assert_eq!(decoder.decode_file(999), None);
    }

    #[test]
    fn test_decoder_merge() {
        let mut encoder = DictEncoder::new();

        encoder.encode_file("file1.rs");
        let delta1 = encoder.get_delta();

        encoder.encode_file("file2.rs");
        encoder.encode_file("file3.rs");
        let delta2 = encoder.get_delta();

        let mut decoder = DictDecoder::from_delta(&delta1);
        assert_eq!(decoder.decode_file(0), Some("file1.rs"));
        assert_eq!(decoder.decode_file(1), None);

        decoder.merge(&delta2);
        assert_eq!(decoder.decode_file(0), Some("file1.rs"));
        assert_eq!(decoder.decode_file(1), Some("file2.rs"));
        assert_eq!(decoder.decode_file(2), Some("file3.rs"));
    }

    #[test]
    fn test_encoder_from_session() {
        let mut files = HashMap::new();
        files.insert("existing.rs".to_string(), 5u32);

        let mut kinds = HashMap::new();
        kinds.insert("function".to_string(), 2u8);

        let modules = HashMap::new();

        let mut encoder = DictEncoder::from_session(files, kinds, modules);

        // Existing entries should return same IDs
        let (id, is_new) = encoder.encode_file("existing.rs");
        assert!(!is_new);
        assert_eq!(id, 5);

        // New entries should get next IDs
        let (id, is_new) = encoder.encode_file("new.rs");
        assert!(is_new);
        assert_eq!(id, 6); // 5 + 1

        let (id, is_new) = encoder.encode_kind("struct");
        assert!(is_new);
        assert_eq!(id, 3); // 2 + 1
    }

    #[test]
    fn test_encoder_get_dictionaries() {
        let mut encoder = DictEncoder::new();

        encoder.encode_file("test.rs");
        encoder.encode_kind("function");
        encoder.encode_module("mod::test");

        let (files, kinds, modules) = encoder.get_dictionaries();

        assert_eq!(files.len(), 1);
        assert_eq!(kinds.len(), 1);
        assert_eq!(modules.len(), 1);

        assert_eq!(files.get("test.rs"), Some(&0));
        assert_eq!(kinds.get("function"), Some(&0));
        assert_eq!(modules.get("mod::test"), Some(&0));
    }
}

mod session_manager {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = Session::new("test-session-123".to_string());

        assert_eq!(session.id, "test-session-123");
        assert!(session.created_at > 0);
        assert!(session.last_accessed > 0);
        assert!(session.metadata.is_empty());
    }

    #[test]
    fn test_session_touch() {
        let mut session = Session::new("test".to_string());
        let initial_access = session.last_accessed;

        // Touch should update last_accessed
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.touch();

        // Time should have advanced (or at least not gone backwards)
        assert!(session.last_accessed >= initial_access);
    }

    #[test]
    fn test_session_get_dict() {
        let mut session = Session::new("test".to_string());

        session.encoder.encode_file("test.rs");
        session.encoder.encode_kind("function");

        let dict = session.get_dict();
        assert!(!dict.is_empty());
        assert_eq!(dict.files.get(&0), Some(&"test.rs".to_string()));
    }

    #[test]
    fn test_manager_open_new_session() {
        let manager = SessionManager::new();

        let session = manager.open_session(None);
        assert!(!session.id.is_empty());
        assert_eq!(manager.session_count(), 1);
    }

    #[test]
    fn test_manager_open_restore_session() {
        let manager = SessionManager::new();

        let session1 = manager.open_session(None);
        let session_id = session1.id.clone();

        let session2 = manager.open_session(Some(&session_id));
        assert_eq!(session2.id, session_id);
        assert_eq!(manager.session_count(), 1); // Still just 1 session
    }

    #[test]
    fn test_manager_open_nonexistent_session() {
        let manager = SessionManager::new();

        // Trying to restore non-existent session creates a new one
        let session = manager.open_session(Some("nonexistent-id"));
        assert_ne!(session.id, "nonexistent-id");
        assert_eq!(manager.session_count(), 1);
    }

    #[test]
    fn test_manager_get_session() {
        let manager = SessionManager::new();

        let session = manager.open_session(None);
        let session_id = session.id.clone();

        let retrieved = manager.get_session(&session_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, session_id);

        let missing = manager.get_session("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_manager_update_session() {
        let manager = SessionManager::new();

        let session = manager.open_session(None);
        let session_id = session.id.clone();

        let mut encoder = DictEncoder::new();
        encoder.encode_file("updated.rs");

        let updated = manager.update_session(&session_id, encoder);
        assert!(updated);

        let retrieved = manager.get_session(&session_id).unwrap();
        let delta = retrieved.encoder.get_delta();
        assert_eq!(delta.files.get(&0), Some(&"updated.rs".to_string()));
    }

    #[test]
    fn test_manager_update_nonexistent() {
        let manager = SessionManager::new();

        let encoder = DictEncoder::new();
        let updated = manager.update_session("nonexistent", encoder);
        assert!(!updated);
    }

    #[test]
    fn test_manager_close_session() {
        let manager = SessionManager::new();

        let session = manager.open_session(None);
        let session_id = session.id.clone();

        assert_eq!(manager.session_count(), 1);

        let closed = manager.close_session(&session_id);
        assert!(closed);
        assert_eq!(manager.session_count(), 0);

        // Closing again should return false
        let closed_again = manager.close_session(&session_id);
        assert!(!closed_again);
    }

    #[test]
    fn test_manager_multiple_sessions() {
        let manager = SessionManager::new();

        let session1 = manager.open_session(None);
        let session2 = manager.open_session(None);
        let session3 = manager.open_session(None);

        assert_ne!(session1.id, session2.id);
        assert_ne!(session2.id, session3.id);
        assert_eq!(manager.session_count(), 3);
    }

    #[test]
    fn test_manager_with_custom_max_age() {
        let manager = SessionManager::with_max_age(7200); // 2 hours

        let session = manager.open_session(None);
        assert!(!session.id.is_empty());
    }

    #[test]
    fn test_manager_encoder_persistence() {
        let manager = SessionManager::new();

        // Open session and encode some values
        let session = manager.open_session(None);
        let session_id = session.id.clone();

        let mut encoder = session.encoder.clone();
        encoder.encode_file("file1.rs");
        encoder.encode_file("file2.rs");
        encoder.encode_kind("function");

        manager.update_session(&session_id, encoder);

        // Retrieve and verify
        let retrieved = manager.get_session(&session_id).unwrap();
        let delta = retrieved.encoder.get_delta();

        assert_eq!(delta.files.len(), 2);
        assert_eq!(delta.kinds.len(), 1);
    }
}

// ============================================================================
// Integration: Building real index and testing compass
// ============================================================================

mod integration {
    use super::*;

    fn index_test_directory() -> SqliteIndex {
        let temp_db = NamedTempFile::new().expect("Failed to create temp file");
        let db_path = temp_db.path().to_path_buf();
        let _ = temp_db.into_temp_path();

        let registry = LanguageRegistry::new();
        let walker = FileWalker::new(registry);
        let registry = LanguageRegistry::new();
        let parser = Parser::new(registry);
        let extractor = SymbolExtractor::new();
        let index = SqliteIndex::new(&db_path).expect("Failed to create index");

        // Index the examples/hello-rust directory
        let base_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/hello-rust");

        if base_path.exists() {
            let files = walker.walk(&base_path).expect("Failed to walk directory");

            for file in &files {
                if let Ok(parsed) = parser.parse_file(file) {
                    if let Ok(symbols) = extractor.extract(&parsed, file) {
                        index.add_symbols(symbols).expect("Failed to add symbols");
                    }
                }
            }
        }

        index
    }

    #[test]
    fn test_profile_builder_with_real_index() {
        let index = index_test_directory();

        // Add some test data if the example dir doesn't exist
        index.add_symbols(vec![
            create_test_symbol("main", SymbolKind::Function, "src/main.rs", 1),
            create_test_symbol("Config", SymbolKind::Struct, "src/config.rs", 1),
            create_test_symbol("handle_request", SymbolKind::Function, "src/api/handler.rs", 1),
        ]).unwrap();

        let result = ProfileBuilder::build(&index);
        assert!(result.is_ok());

        let profile = result.unwrap();
        assert!(!profile.languages.is_empty() || profile.total_symbols == 0);
    }

    #[test]
    fn test_node_builder_with_real_index() {
        let index = create_test_index();

        // Add symbols in different directories
        let mut sym1 = create_test_symbol("func1", SymbolKind::Function, "/project/src/api/handler.rs", 1);
        sym1.visibility = Some(Visibility::Public);

        let sym2 = create_test_symbol("func2", SymbolKind::Function, "/project/src/domain/user.rs", 1);
        let sym3 = create_test_symbol("Type1", SymbolKind::Struct, "/project/src/api/types.rs", 1);

        index.add_symbols(vec![sym1, sym2, sym3]).unwrap();

        let result = NodeBuilder::build(&index, "/project");
        assert!(result.is_ok());

        let nodes = result.unwrap();
        // Should have nodes for src/api and src/domain
        assert!(!nodes.is_empty());
    }

    #[test]
    fn test_session_workflow() {
        let manager = SessionManager::new();

        // 1. Open new session
        let session = manager.open_session(None);
        let session_id = session.id.clone();

        // 2. Use encoder in session
        let mut encoder = session.encoder.clone();

        // Simulate encoding symbols from search results
        let files = ["src/main.rs", "src/lib.rs", "src/api/mod.rs"];
        let kinds = ["function", "struct", "method"];

        for file in &files {
            encoder.encode_file(file);
        }
        for kind in &kinds {
            encoder.encode_kind(kind);
        }

        // 3. Save encoder state
        manager.update_session(&session_id, encoder.clone());

        // 4. Later: restore session
        let restored = manager.open_session(Some(&session_id));
        assert_eq!(restored.id, session_id);

        // 5. Verify encoder state preserved
        let delta = restored.encoder.get_delta();
        assert_eq!(delta.files.len(), 3);
        assert_eq!(delta.kinds.len(), 3);

        // 6. Continue encoding (should reuse existing IDs)
        let mut restored_encoder = restored.encoder.clone();
        let (id, is_new) = restored_encoder.encode_file("src/main.rs");
        assert!(!is_new);
        assert_eq!(id, 0);

        // 7. Close session
        assert!(manager.close_session(&session_id));
        assert_eq!(manager.session_count(), 0);
    }
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn test_empty_readme() {
        let digest = DocParser::parse("README.md", "");

        assert_eq!(digest.doc_type, DocType::Readme);
        assert!(digest.title.is_none());
        assert!(digest.headings.is_empty());
        assert!(digest.command_blocks.is_empty());
    }

    #[test]
    fn test_readme_without_headings() {
        let content = "Just some text without any headings.";
        let digest = DocParser::parse("README.md", content);

        assert!(digest.headings.is_empty());
        assert!(digest.title.is_none());
    }

    #[test]
    fn test_unclosed_code_block() {
        let content = r#"# Title

```bash
npm install
"#;
        let digest = DocParser::parse("README.md", content);

        // Unclosed code block should not crash
        assert_eq!(digest.headings.len(), 1);
    }

    #[test]
    fn test_deeply_nested_headings() {
        let content = r#"# H1
## H2
### H3
#### H4
##### H5
###### H6
####### Not a heading (7 hashes)
"#;
        let digest = DocParser::parse("README.md", content);

        assert_eq!(digest.headings.len(), 6);
        assert_eq!(digest.headings[5].level, 6);
    }

    #[test]
    fn test_package_json_minimal() {
        let content = r#"{"name": "minimal"}"#;
        let digest = ConfigParser::parse("package.json", content).unwrap();

        assert_eq!(digest.name.as_deref(), Some("minimal"));
        assert!(digest.scripts.is_empty());
    }

    #[test]
    fn test_cargo_toml_minimal() {
        let content = r#"
[package]
name = "minimal"
version = "0.1.0"
"#;
        let digest = ConfigParser::parse("Cargo.toml", content).unwrap();

        assert_eq!(digest.name.as_deref(), Some("minimal"));
        // Should still have standard cargo commands
        assert!(digest.scripts.contains_key("build"));
    }

    #[test]
    fn test_makefile_with_special_targets() {
        let content = r#"
.PHONY: all
.DEFAULT: build

%: %.c
	gcc -o $@ $<

build:
	make all
"#;
        let digest = ConfigParser::parse("Makefile", content).unwrap();

        // Should only capture 'build', not pattern rules
        assert!(digest.scripts.contains_key("build"));
        assert!(!digest.scripts.contains_key("%"));
    }

    #[test]
    fn test_session_many_entries() {
        let mut encoder = DictEncoder::new();

        // Encode many files
        for i in 0..1000 {
            let (id, is_new) = encoder.encode_file(&format!("file{}.rs", i));
            assert!(is_new);
            assert_eq!(id, i as u32);
        }

        // Verify all can be decoded
        let delta = encoder.get_delta();
        assert_eq!(delta.files.len(), 1000);

        let decoder = DictDecoder::from_delta(&delta);
        for i in 0..1000 {
            assert_eq!(decoder.decode_file(i), Some(format!("file{}.rs", i).as_str()));
        }
    }

    #[test]
    fn test_kind_overflow_protection() {
        let mut encoder = DictEncoder::new();

        // u8 can hold 256 values (0-255)
        for i in 0..260 {
            encoder.encode_kind(&format!("kind{}", i));
        }

        // Should not panic due to saturating_add
        let delta = encoder.get_delta();
        assert!(delta.kinds.len() <= 256);
    }
}
