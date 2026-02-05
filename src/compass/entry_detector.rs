//! Entry Point Detector
//!
//! Detects application entry points like main functions, server handlers,
//! CLI commands, and API routes.

use serde::{Deserialize, Serialize};

use crate::index::{CodeIndex, SearchOptions, SymbolKind};

/// Type of entry point
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    /// Standard main function
    Main,
    /// Tokio async main
    TokioMain,
    /// Actix-web main
    ActixMain,
    /// HTTP server/listener
    Server,
    /// CLI command handler
    Cli,
    /// REST API endpoint
    RestEndpoint,
    /// GraphQL resolver
    GraphqlResolver,
    /// gRPC service
    GrpcService,
    /// Test entry
    Test,
    /// Benchmark entry
    Benchmark,
}

impl EntryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryType::Main => "main",
            EntryType::TokioMain => "tokio_main",
            EntryType::ActixMain => "actix_main",
            EntryType::Server => "server",
            EntryType::Cli => "cli",
            EntryType::RestEndpoint => "rest_endpoint",
            EntryType::GraphqlResolver => "graphql_resolver",
            EntryType::GrpcService => "grpc_service",
            EntryType::Test => "test",
            EntryType::Benchmark => "benchmark",
        }
    }
}

/// A detected entry point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPoint {
    /// Symbol ID (if available)
    pub symbol_id: Option<String>,
    /// Entry type
    pub entry_type: EntryType,
    /// File path
    pub file_path: String,
    /// Line number
    pub line: u32,
    /// Entry point name
    pub name: String,
    /// Evidence of why this is an entry point
    pub evidence: String,
}

/// Detector for entry points
pub struct EntryDetector;

impl EntryDetector {
    /// Detect all entry points in the index
    pub fn detect(index: &dyn CodeIndex) -> crate::error::Result<Vec<EntryPoint>> {
        let mut entries = Vec::new();

        // Detect main functions
        entries.extend(Self::detect_main_functions(index)?);

        // Detect async mains (tokio, actix)
        entries.extend(Self::detect_async_mains(index)?);

        // Detect server listeners
        entries.extend(Self::detect_servers(index)?);

        // Detect CLI handlers
        entries.extend(Self::detect_cli_handlers(index)?);

        // Detect REST endpoints
        entries.extend(Self::detect_rest_endpoints(index)?);

        // Sort by entry type priority and then by file path
        entries.sort_by(|a, b| {
            let type_order = |t: &EntryType| match t {
                EntryType::Main | EntryType::TokioMain | EntryType::ActixMain => 0,
                EntryType::Server => 1,
                EntryType::Cli => 2,
                EntryType::RestEndpoint => 3,
                _ => 4,
            };
            type_order(&a.entry_type)
                .cmp(&type_order(&b.entry_type))
                .then_with(|| a.file_path.cmp(&b.file_path))
        });

        // Deduplicate by file+line
        entries.dedup_by(|a, b| a.file_path == b.file_path && a.line == b.line);

        Ok(entries)
    }

    fn detect_main_functions(index: &dyn CodeIndex) -> crate::error::Result<Vec<EntryPoint>> {
        let mut entries = Vec::new();

        let options = SearchOptions {
            limit: Some(100),
            kind_filter: Some(vec![SymbolKind::Function]),
            ..Default::default()
        };

        // Search for "main" functions
        if let Ok(results) = index.search("main", &options) {
            for result in results {
                let symbol = &result.symbol;
                if symbol.name == "main" && symbol.kind == SymbolKind::Function {
                    // Check if it's in a main.rs, main.py, main.go, etc.
                    let is_main_file = symbol.location.file_path.contains("main.")
                        || symbol.location.file_path.ends_with("/main.rs")
                        || symbol.location.file_path.ends_with("/main.py")
                        || symbol.location.file_path.ends_with("/main.go")
                        || symbol.location.file_path.ends_with("/Main.java")
                        || symbol.location.file_path.ends_with("/Main.kt");

                    if is_main_file {
                        entries.push(EntryPoint {
                            symbol_id: Some(symbol.id.clone()),
                            entry_type: EntryType::Main,
                            file_path: symbol.location.file_path.clone(),
                            line: symbol.location.start_line,
                            name: "main".to_string(),
                            evidence: "fn main() in main file".to_string(),
                        });
                    }
                }
            }
        }

        Ok(entries)
    }

    fn detect_async_mains(index: &dyn CodeIndex) -> crate::error::Result<Vec<EntryPoint>> {
        let mut entries = Vec::new();

        let options = SearchOptions {
            limit: Some(100),
            ..Default::default()
        };

        // Search for tokio::main
        if let Ok(results) = index.search("tokio_main", &options) {
            for result in results {
                let symbol = &result.symbol;
                if symbol.kind == SymbolKind::Function {
                    entries.push(EntryPoint {
                        symbol_id: Some(symbol.id.clone()),
                        entry_type: EntryType::TokioMain,
                        file_path: symbol.location.file_path.clone(),
                        line: symbol.location.start_line,
                        name: symbol.name.clone(),
                        evidence: "#[tokio::main] attribute".to_string(),
                    });
                }
            }
        }

        // Search for actix_web::main
        if let Ok(results) = index.search("actix_web_main", &options) {
            for result in results {
                let symbol = &result.symbol;
                if symbol.kind == SymbolKind::Function {
                    entries.push(EntryPoint {
                        symbol_id: Some(symbol.id.clone()),
                        entry_type: EntryType::ActixMain,
                        file_path: symbol.location.file_path.clone(),
                        line: symbol.location.start_line,
                        name: symbol.name.clone(),
                        evidence: "#[actix_web::main] attribute".to_string(),
                    });
                }
            }
        }

        Ok(entries)
    }

    fn detect_servers(index: &dyn CodeIndex) -> crate::error::Result<Vec<EntryPoint>> {
        let mut entries = Vec::new();

        let options = SearchOptions {
            limit: Some(50),
            ..Default::default()
        };

        // Common server patterns
        let patterns = [
            ("listen", "server.listen() call"),
            ("serve", "serve() call"),
            ("bind", "server.bind() call"),
            ("run_server", "run_server function"),
            ("start_server", "start_server function"),
        ];

        for (pattern, evidence) in patterns {
            if let Ok(results) = index.search(pattern, &options) {
                for result in results {
                    let symbol = &result.symbol;
                    if symbol.kind == SymbolKind::Function
                        && (symbol.name.contains("server") || symbol.name.contains("listen"))
                    {
                        entries.push(EntryPoint {
                            symbol_id: Some(symbol.id.clone()),
                            entry_type: EntryType::Server,
                            file_path: symbol.location.file_path.clone(),
                            line: symbol.location.start_line,
                            name: symbol.name.clone(),
                            evidence: evidence.to_string(),
                        });
                    }
                }
            }
        }

        Ok(entries)
    }

    fn detect_cli_handlers(index: &dyn CodeIndex) -> crate::error::Result<Vec<EntryPoint>> {
        let mut entries = Vec::new();

        let options = SearchOptions {
            limit: Some(100),
            kind_filter: Some(vec![SymbolKind::Function]),
            ..Default::default()
        };

        // CLI patterns
        let patterns = [
            ("command", "CLI command handler"),
            ("subcommand", "CLI subcommand handler"),
            ("execute", "execute command"),
            ("run_cli", "CLI runner"),
        ];

        for (pattern, evidence) in patterns {
            if let Ok(results) = index.search(pattern, &options) {
                for result in results {
                    let symbol = &result.symbol;
                    // Check if in CLI-related path
                    let is_cli_file = symbol.location.file_path.contains("cli")
                        || symbol.location.file_path.contains("command")
                        || symbol.location.file_path.contains("cmd");

                    if is_cli_file && symbol.kind == SymbolKind::Function {
                        entries.push(EntryPoint {
                            symbol_id: Some(symbol.id.clone()),
                            entry_type: EntryType::Cli,
                            file_path: symbol.location.file_path.clone(),
                            line: symbol.location.start_line,
                            name: symbol.name.clone(),
                            evidence: evidence.to_string(),
                        });
                    }
                }
            }
        }

        Ok(entries)
    }

    fn detect_rest_endpoints(index: &dyn CodeIndex) -> crate::error::Result<Vec<EntryPoint>> {
        let mut entries = Vec::new();

        let options = SearchOptions {
            limit: Some(200),
            kind_filter: Some(vec![SymbolKind::Function]),
            ..Default::default()
        };

        // REST handler patterns
        let patterns = [
            ("get_", "GET endpoint"),
            ("post_", "POST endpoint"),
            ("put_", "PUT endpoint"),
            ("delete_", "DELETE endpoint"),
            ("patch_", "PATCH endpoint"),
            ("handler", "request handler"),
        ];

        for (pattern, evidence) in patterns {
            if let Ok(results) = index.search(pattern, &options) {
                for result in results {
                    let symbol = &result.symbol;
                    // Check if in API-related path
                    let is_api_file = symbol.location.file_path.contains("api")
                        || symbol.location.file_path.contains("handler")
                        || symbol.location.file_path.contains("route")
                        || symbol.location.file_path.contains("controller");

                    if is_api_file && symbol.kind == SymbolKind::Function {
                        entries.push(EntryPoint {
                            symbol_id: Some(symbol.id.clone()),
                            entry_type: EntryType::RestEndpoint,
                            file_path: symbol.location.file_path.clone(),
                            line: symbol.location.start_line,
                            name: symbol.name.clone(),
                            evidence: evidence.to_string(),
                        });
                    }
                }
            }
        }

        // Limit to top endpoints
        entries.truncate(20);

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_type_as_str() {
        assert_eq!(EntryType::Main.as_str(), "main");
        assert_eq!(EntryType::TokioMain.as_str(), "tokio_main");
        assert_eq!(EntryType::Server.as_str(), "server");
    }
}
