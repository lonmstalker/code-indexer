use std::fs;
use std::path::Path;
use std::sync::Arc;

use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool, ToolsCapability,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::index::sqlite::SqliteIndex;
use crate::index::{CodeIndex, SearchOptions, SymbolKind};

#[derive(Clone)]
pub struct McpServer {
    index: Arc<SqliteIndex>,
}

impl McpServer {
    pub fn new(index: Arc<SqliteIndex>) -> Self {
        Self { index }
    }

    // === Workspace helper methods ===

    fn list_modules_impl(&self, workspace_path: &str) -> crate::error::Result<String> {
        use code_indexer::workspace::WorkspaceDetector;

        let path = std::path::Path::new(workspace_path);
        let workspace = WorkspaceDetector::parse(path)?;

        let output = serde_json::json!({
            "workspace_type": workspace.workspace_type.as_str(),
            "name": workspace.name,
            "root_path": workspace.root_path.to_string_lossy(),
            "modules": workspace.modules.iter().map(|m| {
                serde_json::json!({
                    "name": m.name,
                    "path": m.path.to_string_lossy(),
                    "language": m.language,
                    "type": m.module_type.as_ref().map(|t| t.as_str()),
                    "internal_dependencies": m.internal_dependencies,
                })
            }).collect::<Vec<_>>(),
        });

        Ok(serde_json::to_string_pretty(&output).unwrap_or_default())
    }

    fn get_module_info_impl(
        &self,
        workspace_path: &str,
        module_name: &str,
    ) -> crate::error::Result<String> {
        use code_indexer::workspace::WorkspaceDetector;

        let path = std::path::Path::new(workspace_path);
        let workspace = WorkspaceDetector::parse(path)?;

        let module = workspace.get_module(module_name).ok_or_else(|| {
            crate::error::IndexerError::Index(format!("Module '{}' not found", module_name))
        })?;

        let module_path = if module.path.is_absolute() {
            module.path.clone()
        } else {
            workspace.root_path.join(&module.path)
        };

        // Get symbol stats for this module
        let file_filter = module_path.to_string_lossy().to_string();
        let options = SearchOptions {
            file_filter: Some(file_filter),
            ..Default::default()
        };

        let functions = self.index.list_functions(&options).unwrap_or_default();
        let types = self.index.list_types(&options).unwrap_or_default();

        let output = serde_json::json!({
            "name": module.name,
            "path": module.path.to_string_lossy(),
            "absolute_path": module_path.to_string_lossy(),
            "language": module.language,
            "type": module.module_type.as_ref().map(|t| t.as_str()),
            "internal_dependencies": module.internal_dependencies,
            "stats": {
                "functions": functions.len(),
                "types": types.len(),
            }
        });

        Ok(serde_json::to_string_pretty(&output).unwrap_or_default())
    }

    fn find_module_dependencies_impl(
        &self,
        workspace_path: &str,
        module_name: &str,
    ) -> crate::error::Result<String> {
        use code_indexer::workspace::WorkspaceDetector;

        let path = std::path::Path::new(workspace_path);
        let workspace = WorkspaceDetector::parse(path)?;

        let module = workspace.get_module(module_name).ok_or_else(|| {
            crate::error::IndexerError::Index(format!("Module '{}' not found", module_name))
        })?;

        // Find modules that depend on this one
        let dependents: Vec<_> = workspace
            .modules
            .iter()
            .filter(|m| m.internal_dependencies.contains(&module_name.to_string()))
            .map(|m| m.name.clone())
            .collect();

        let output = serde_json::json!({
            "module": module_name,
            "depends_on": module.internal_dependencies,
            "depended_by": dependents,
        });

        Ok(serde_json::to_string_pretty(&output).unwrap_or_default())
    }

    fn search_in_module_impl(
        &self,
        workspace_path: &str,
        module_name: &str,
        query: &str,
        limit: Option<usize>,
    ) -> crate::error::Result<String> {
        use code_indexer::workspace::WorkspaceDetector;

        let path = std::path::Path::new(workspace_path);
        let workspace = WorkspaceDetector::parse(path)?;

        let module = workspace.get_module(module_name).ok_or_else(|| {
            crate::error::IndexerError::Index(format!("Module '{}' not found", module_name))
        })?;

        let module_path = if module.path.is_absolute() {
            module.path.clone()
        } else {
            workspace.root_path.join(&module.path)
        };

        let file_filter = module_path.to_string_lossy().to_string();
        let options = SearchOptions {
            limit,
            file_filter: Some(file_filter),
            ..Default::default()
        };

        let results = self.index.search(query, &options)?;

        Ok(serde_json::to_string_pretty(&results).unwrap_or_default())
    }

    // === Memory Bank / Project Context methods ===

    fn get_project_context_impl(&self, project_path: &str) -> crate::error::Result<String> {
        use code_indexer::memory::ArchitectureAnalyzer;

        let path = std::path::Path::new(project_path);
        let context = ArchitectureAnalyzer::analyze(path, self.index.as_ref())?;

        Ok(serde_json::to_string_pretty(&context).unwrap_or_default())
    }

    fn get_architecture_summary_impl(&self, project_path: &str) -> crate::error::Result<String> {
        use code_indexer::memory::ArchitectureAnalyzer;

        let path = std::path::Path::new(project_path);
        let context = ArchitectureAnalyzer::analyze(path, self.index.as_ref())?;

        Ok(serde_json::to_string_pretty(&context.architecture).unwrap_or_default())
    }

    // === Cross-language methods ===

    fn find_cross_language_refs_impl(
        &self,
        symbol_name: &str,
        source_language: Option<&str>,
        target_language: Option<&str>,
    ) -> crate::error::Result<String> {
        use code_indexer::languages::CrossLanguageAnalyzer;

        let analyzer = CrossLanguageAnalyzer::new();
        let refs = analyzer.find_cross_language_refs(
            self.index.as_ref(),
            symbol_name,
            source_language,
            target_language,
        )?;

        Ok(serde_json::to_string_pretty(&refs).unwrap_or_default())
    }

    fn find_kotlin_extensions_impl(&self, java_type: &str) -> crate::error::Result<String> {
        use code_indexer::languages::CrossLanguageAnalyzer;

        let analyzer = CrossLanguageAnalyzer::new();
        let extensions = analyzer.find_kotlin_extensions(self.index.as_ref(), java_type)?;

        Ok(serde_json::to_string_pretty(&extensions).unwrap_or_default())
    }

    fn list_dependencies_impl(
        &self,
        project_path: &str,
        include_dev: bool,
    ) -> crate::error::Result<String> {
        use code_indexer::dependencies::DependencyRegistry;

        let registry = DependencyRegistry::with_defaults();
        let path = std::path::Path::new(project_path);

        // Find and parse manifest
        let project = if path.is_file() {
            registry.parse_manifest(path)?
        } else if let Some(ecosystem) = registry.detect_ecosystem(path) {
            let manifest_name = ecosystem.manifest_names()[0];
            let manifest_path = path.join(manifest_name);
            registry.parse_manifest(&manifest_path)?
        } else {
            return Err(crate::error::IndexerError::FileNotFound(
                "No manifest file found".to_string(),
            ));
        };

        // Store in database
        let project_id = self.index.add_project(&project)?;
        self.index
            .add_dependencies(project_id, &project.dependencies)?;

        // Filter and return
        let deps: Vec<_> = if include_dev {
            project.dependencies
        } else {
            project
                .dependencies
                .into_iter()
                .filter(|d| !d.is_dev)
                .collect()
        };

        let output = serde_json::json!({
            "project": {
                "name": project.name,
                "version": project.version,
                "ecosystem": project.ecosystem.as_str(),
            },
            "dependencies": deps,
        });

        Ok(serde_json::to_string_pretty(&output).unwrap_or_default())
    }

    fn get_dependency_source_impl(
        &self,
        symbol_name: &str,
        dependency: Option<&str>,
        context_lines: usize,
    ) -> crate::error::Result<String> {
        let symbols = self
            .index
            .find_definition_in_dependencies(symbol_name, dependency)?;

        if symbols.is_empty() {
            return Ok(format!(
                "No definition found for '{}' in dependencies",
                symbol_name
            ));
        }

        let mut output = String::new();

        for symbol in symbols {
            output.push_str(&format!(
                "=== {} ({}) ===\n",
                symbol.name,
                symbol.kind.as_str()
            ));
            output.push_str(&format!("File: {}\n", symbol.location.file_path));

            let file_path = Path::new(&symbol.location.file_path);
            if !file_path.exists() {
                output.push_str(&format!(
                    "Source file not found: {}\n\n",
                    symbol.location.file_path
                ));
                continue;
            }

            match fs::read_to_string(file_path) {
                Ok(content) => {
                    let lines: Vec<&str> = content.lines().collect();
                    let start = symbol.location.start_line.saturating_sub(1) as usize;
                    let end = symbol.location.end_line as usize;

                    let ctx_start = start.saturating_sub(context_lines);
                    let ctx_end = (end + context_lines).min(lines.len());

                    output.push_str(&format!("Lines {}-{}:\n", ctx_start + 1, ctx_end));
                    output.push_str("---\n");

                    for (i, line) in lines[ctx_start..ctx_end].iter().enumerate() {
                        let line_num = ctx_start + i + 1;
                        let marker = if line_num >= start + 1 && line_num <= end {
                            ">"
                        } else {
                            " "
                        };
                        output.push_str(&format!("{} {:4} | {}\n", marker, line_num, line));
                    }
                    output.push_str("---\n\n");
                }
                Err(e) => {
                    output.push_str(&format!("Error reading file: {}\n\n", e));
                }
            }
        }

        Ok(output)
    }

    fn search_by_pattern_impl(
        &self,
        pattern: &str,
        file_glob: Option<&str>,
        limit: Option<usize>,
    ) -> crate::error::Result<String> {
        use regex::Regex;

        let regex = Regex::new(pattern).map_err(|e| {
            crate::error::IndexerError::Index(format!("Invalid regex pattern: {}", e))
        })?;

        let limit = limit.unwrap_or(100);

        // Get all symbols and filter by pattern
        let options = SearchOptions {
            limit: Some(limit * 10), // Get more to filter
            file_filter: file_glob.map(|g| g.replace("*", "")),
            ..Default::default()
        };

        // Search all functions and types
        let mut results = Vec::new();

        if let Ok(functions) = self.index.list_functions(&options) {
            for symbol in functions {
                if regex.is_match(&symbol.name) {
                    results.push(symbol);
                    if results.len() >= limit {
                        break;
                    }
                }
            }
        }

        if results.len() < limit {
            if let Ok(types) = self.index.list_types(&options) {
                for symbol in types {
                    if regex.is_match(&symbol.name) {
                        results.push(symbol);
                        if results.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }

        Ok(serde_json::to_string_pretty(&results).unwrap_or_default())
    }

    fn get_dependency_info_impl(
        &self,
        project_path: &str,
        dep_name: &str,
    ) -> crate::error::Result<String> {
        use code_indexer::dependencies::DependencyRegistry;

        let registry = DependencyRegistry::with_defaults();
        let path = std::path::Path::new(project_path);

        let project = if path.is_file() {
            registry.parse_manifest(path)?
        } else if let Some(ecosystem) = registry.detect_ecosystem(path) {
            let manifest_name = ecosystem.manifest_names()[0];
            let manifest_path = path.join(manifest_name);
            registry.parse_manifest(&manifest_path)?
        } else {
            return Err(crate::error::IndexerError::FileNotFound(
                "No manifest file found".to_string(),
            ));
        };

        let dep = project
            .dependencies
            .iter()
            .find(|d| d.name == dep_name)
            .ok_or_else(|| {
                crate::error::IndexerError::Index(format!("Dependency '{}' not found", dep_name))
            })?;

        let mut info = serde_json::json!({
            "name": dep.name,
            "version": dep.version,
            "ecosystem": dep.ecosystem.as_str(),
            "is_dev": dep.is_dev,
            "source_available": dep.source_path.is_some(),
        });

        if let Some(ref source_path) = dep.source_path {
            info["source_path"] = serde_json::json!(source_path);
        }

        // Check indexed status
        if let Some(project_id) = self.index.get_project_id(&project.manifest_path)? {
            if let Some(db_dep) = self.index.get_dependency(project_id, dep_name)? {
                info["is_indexed"] = serde_json::json!(db_dep.is_indexed);
            }
        }

        Ok(serde_json::to_string_pretty(&info).unwrap_or_default())
    }
}

fn schema_for<T: JsonSchema>() -> Arc<serde_json::Map<String, serde_json::Value>> {
    let schema = schemars::schema_for!(T);
    let value = serde_json::to_value(&schema).expect("Failed to serialize schema");
    match value {
        serde_json::Value::Object(map) => Arc::new(map),
        _ => Arc::new(serde_json::Map::new()),
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchSymbolParams {
    /// Query
    pub query: String,
    /// Max results
    #[serde(default)]
    pub limit: Option<usize>,
    /// Kind filter
    #[serde(default)]
    pub kind: Option<String>,
    /// Language
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindDefinitionParams {
    /// Symbol name
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListFunctionsParams {
    /// Max results
    #[serde(default)]
    pub limit: Option<usize>,
    /// Language
    #[serde(default)]
    pub language: Option<String>,
    /// File filter
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListTypesParams {
    /// Max results
    #[serde(default)]
    pub limit: Option<usize>,
    /// Language
    #[serde(default)]
    pub language: Option<String>,
    /// File filter
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetFileStructureParams {
    /// File path
    pub file_path: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetSymbolParams {
    /// ID
    pub id: String,
}

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct EmptyParams {}

// === Dependency-related params ===

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListDependenciesParams {
    /// Project path
    #[serde(default)]
    pub project_path: Option<String>,
    /// Include dev
    #[serde(default)]
    pub include_dev: Option<bool>,
}

// Note: IndexDependencyParams is reserved for future use when we add
// MCP-based dependency indexing (currently only CLI-based indexing is supported)

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindInDependencyParams {
    /// Name
    pub name: String,
    /// Dependency
    #[serde(default)]
    pub dependency: Option<String>,
    /// Kind
    #[serde(default)]
    pub kind: Option<String>,
    /// Limit
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetDependencySourceParams {
    /// Symbol
    pub symbol_name: String,
    /// Dependency
    #[serde(default)]
    pub dependency: Option<String>,
    /// Context lines
    #[serde(default)]
    pub context_lines: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DependencyInfoParams {
    /// Name
    pub name: String,
    /// Project path
    #[serde(default)]
    pub project_path: Option<String>,
}

// Note: SearchWithDepsParams is reserved for enhancing search_symbol
// with include_dependencies option in a future update

// === Reference tracking params ===

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindReferencesParams {
    /// Symbol
    pub symbol_name: String,
    /// File filter
    #[serde(default)]
    pub file_filter: Option<String>,
    /// Limit
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindCallersParams {
    /// Function
    pub function_name: String,
    /// Depth
    #[serde(default)]
    pub depth: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindImplementationsParams {
    /// Trait/interface
    pub trait_name: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetSymbolMembersParams {
    /// Type
    pub type_name: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetFileImportsParams {
    /// File path
    pub file_path: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetFileImportersParams {
    /// File path
    pub file_path: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchByPatternParams {
    /// Regex
    pub pattern: String,
    /// File glob
    #[serde(default)]
    pub file_glob: Option<String>,
    /// Limit
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BatchGetSymbolsParams {
    /// IDs
    pub symbol_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindSymbolsInRangeParams {
    /// File
    pub file_path: String,
    /// Start
    pub start_line: u32,
    /// End
    pub end_line: u32,
}

// === Workspace params ===

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListModulesParams {
    /// Path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetModuleInfoParams {
    /// Module
    pub module_name: String,
    /// Path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindModuleDependenciesParams {
    /// Module
    pub module_name: String,
    /// Path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchInModuleParams {
    /// Query
    pub query: String,
    /// Module
    pub module_name: String,
    /// Path
    #[serde(default)]
    pub workspace_path: Option<String>,
    /// Limit
    #[serde(default)]
    pub limit: Option<usize>,
}

// === Memory Bank / Project Context params ===

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetProjectContextParams {
    /// Path
    #[serde(default)]
    pub project_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetArchitectureSummaryParams {
    /// Path
    #[serde(default)]
    pub project_path: Option<String>,
}

// === Cross-language params ===

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindCrossLanguageRefsParams {
    /// Symbol
    pub symbol_name: String,
    /// Source lang
    #[serde(default)]
    pub source_language: Option<String>,
    /// Target lang
    #[serde(default)]
    pub target_language: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindKotlinExtensionsParams {
    /// Java type
    pub java_type: String,
}

// === Analysis params ===

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetCallGraphParams {
    /// Entry point function name
    pub function: String,
    /// Maximum depth of the call graph (default: 3)
    #[serde(default)]
    pub depth: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindCalleesParams {
    /// Function name
    pub function: String,
}

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct FindDeadCodeParams {
    /// Optional path filter
    #[serde(default)]
    pub path_filter: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetMetricsParams {
    /// Function name or file path
    pub target: String,
    /// Whether the target is a file path (default: false, meaning it's a function name)
    #[serde(default)]
    pub is_file: Option<bool>,
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(true),
                }),
                ..Default::default()
            },
            server_info: Implementation {
                name: "code-indexer".to_string(),
                title: Some("Code Indexer".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Code indexer and search tool using tree-sitter. \
                 Provides fast symbol search, definition lookup, and code structure analysis."
                    .to_string(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = vec![
            Tool {
                name: "search_symbol".into(),
                title: Some("Search Symbol".to_string()),
                description: Some("Fuzzy search symbols".into()),
                input_schema: schema_for::<SearchSymbolParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "find_definition".into(),
                title: Some("Find Definition".to_string()),
                description: Some("Find symbol definition".into()),
                input_schema: schema_for::<FindDefinitionParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "list_functions".into(),
                title: Some("List Functions".to_string()),
                description: Some("List functions".into()),
                input_schema: schema_for::<ListFunctionsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "list_types".into(),
                title: Some("List Types".to_string()),
                description: Some("List types".into()),
                input_schema: schema_for::<ListTypesParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "get_file_structure".into(),
                title: Some("Get File Structure".to_string()),
                description: Some("Get file symbols".into()),
                input_schema: schema_for::<GetFileStructureParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "index_stats".into(),
                title: Some("Index Stats".to_string()),
                description: Some("Index stats".into()),
                input_schema: schema_for::<EmptyParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "get_symbol".into(),
                title: Some("Get Symbol".to_string()),
                description: Some("Get symbol by ID".into()),
                input_schema: schema_for::<GetSymbolParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // Dependency tools
            Tool {
                name: "list_dependencies".into(),
                title: Some("List Dependencies".to_string()),
                description: Some("List project deps".into()),
                input_schema: schema_for::<ListDependenciesParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "find_in_dependency".into(),
                title: Some("Find In Dependency".to_string()),
                description: Some("Search in deps".into()),
                input_schema: schema_for::<FindInDependencyParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "get_dependency_source".into(),
                title: Some("Get Dependency Source".to_string()),
                description: Some("Get dep source".into()),
                input_schema: schema_for::<GetDependencySourceParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "dependency_info".into(),
                title: Some("Dependency Info".to_string()),
                description: Some("Dependency info".into()),
                input_schema: schema_for::<DependencyInfoParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // Reference tracking tools
            Tool {
                name: "find_references".into(),
                title: Some("Find References".to_string()),
                description: Some("Find usages".into()),
                input_schema: schema_for::<FindReferencesParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "find_callers".into(),
                title: Some("Find Callers".to_string()),
                description: Some("Find callers".into()),
                input_schema: schema_for::<FindCallersParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "find_implementations".into(),
                title: Some("Find Implementations".to_string()),
                description: Some("Find implementations".into()),
                input_schema: schema_for::<FindImplementationsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "get_symbol_members".into(),
                title: Some("Get Symbol Members".to_string()),
                description: Some("Get type members".into()),
                input_schema: schema_for::<GetSymbolMembersParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "get_file_imports".into(),
                title: Some("Get File Imports".to_string()),
                description: Some("Get imports".into()),
                input_schema: schema_for::<GetFileImportsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "get_file_importers".into(),
                title: Some("Get File Importers".to_string()),
                description: Some("Find importers".into()),
                input_schema: schema_for::<GetFileImportersParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "search_by_pattern".into(),
                title: Some("Search By Pattern".to_string()),
                description: Some("Regex search".into()),
                input_schema: schema_for::<SearchByPatternParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "batch_get_symbols".into(),
                title: Some("Batch Get Symbols".to_string()),
                description: Some("Batch get symbols".into()),
                input_schema: schema_for::<BatchGetSymbolsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "find_symbols_in_range".into(),
                title: Some("Find Symbols In Range".to_string()),
                description: Some("Symbols in range".into()),
                input_schema: schema_for::<FindSymbolsInRangeParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // Workspace tools
            Tool {
                name: "list_modules".into(),
                title: Some("List Modules".to_string()),
                description: Some("List modules".into()),
                input_schema: schema_for::<ListModulesParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "get_module_info".into(),
                title: Some("Get Module Info".to_string()),
                description: Some("Module info".into()),
                input_schema: schema_for::<GetModuleInfoParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "find_module_dependencies".into(),
                title: Some("Find Module Dependencies".to_string()),
                description: Some("Module deps".into()),
                input_schema: schema_for::<FindModuleDependenciesParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "search_in_module".into(),
                title: Some("Search In Module".to_string()),
                description: Some("Search in module".into()),
                input_schema: schema_for::<SearchInModuleParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // Memory Bank / Project Context tools
            Tool {
                name: "get_project_context".into(),
                title: Some("Get Project Context".to_string()),
                description: Some("Project context".into()),
                input_schema: schema_for::<GetProjectContextParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "get_architecture_summary".into(),
                title: Some("Get Architecture Summary".to_string()),
                description: Some("Architecture summary".into()),
                input_schema: schema_for::<GetArchitectureSummaryParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // Cross-language tools
            Tool {
                name: "find_cross_language_refs".into(),
                title: Some("Find Cross-Language References".to_string()),
                description: Some("Cross-lang refs".into()),
                input_schema: schema_for::<FindCrossLanguageRefsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "find_kotlin_extensions".into(),
                title: Some("Find Kotlin Extensions".to_string()),
                description: Some("Kotlin extensions".into()),
                input_schema: schema_for::<FindKotlinExtensionsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // Analysis tools
            Tool {
                name: "get_call_graph".into(),
                title: Some("Get Call Graph".to_string()),
                description: Some("Build call graph from entry point".into()),
                input_schema: schema_for::<GetCallGraphParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "find_callees".into(),
                title: Some("Find Callees".to_string()),
                description: Some("Find functions called by a function".into()),
                input_schema: schema_for::<FindCalleesParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "find_dead_code".into(),
                title: Some("Find Dead Code".to_string()),
                description: Some("Find unused functions and types".into()),
                input_schema: schema_for::<FindDeadCodeParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "get_metrics".into(),
                title: Some("Get Metrics".to_string()),
                description: Some("Get code metrics for function or file".into()),
                input_schema: schema_for::<GetMetricsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
        ];

        Ok(ListToolsResult {
            next_cursor: None,
            tools,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let result = match request.name.as_ref() {
            "search_symbol" => {
                let params: SearchSymbolParams =
                    serde_json::from_value(serde_json::Value::Object(
                        request.arguments.unwrap_or_default(),
                    ))
                    .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let kind_filter = params
                    .kind
                    .and_then(|k| SymbolKind::from_str(&k).map(|kind| vec![kind]));
                let language_filter = params.language.map(|l| vec![l]);
                let options = SearchOptions {
                    limit: params.limit,
                    kind_filter,
                    language_filter,
                    file_filter: None,
                    name_filter: None,
                };

                match self.index.search(&params.query, &options) {
                    Ok(results) => {
                        let json = serde_json::to_string_pretty(&results).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "find_definition" => {
                let params: FindDefinitionParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.find_definition(&params.name) {
                    Ok(symbols) => {
                        let json = serde_json::to_string_pretty(&symbols).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "list_functions" => {
                let params: ListFunctionsParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let options = SearchOptions {
                    limit: params.limit,
                    kind_filter: None,
                    language_filter: params.language.map(|l| vec![l]),
                    file_filter: params.file,
                    name_filter: None,
                };

                match self.index.list_functions(&options) {
                    Ok(symbols) => {
                        let json = serde_json::to_string_pretty(&symbols).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "list_types" => {
                let params: ListTypesParams = serde_json::from_value(serde_json::Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let options = SearchOptions {
                    limit: params.limit,
                    kind_filter: None,
                    language_filter: params.language.map(|l| vec![l]),
                    file_filter: params.file,
                    name_filter: None,
                };

                match self.index.list_types(&options) {
                    Ok(symbols) => {
                        let json = serde_json::to_string_pretty(&symbols).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "get_file_structure" => {
                let params: GetFileStructureParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.get_file_symbols(&params.file_path) {
                    Ok(symbols) => {
                        let json = serde_json::to_string_pretty(&symbols).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "index_stats" => match self.index.get_stats() {
                Ok(stats) => {
                    let json = serde_json::to_string_pretty(&stats).unwrap_or_default();
                    CallToolResult::success(vec![Content::text(json)])
                }
                Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
            },
            "get_symbol" => {
                let params: GetSymbolParams = serde_json::from_value(serde_json::Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.get_symbol(&params.id) {
                    Ok(Some(symbol)) => {
                        let json = serde_json::to_string_pretty(&symbol).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Ok(None) => {
                        CallToolResult::error(vec![Content::text(format!(
                            "Symbol not found: {}",
                            params.id
                        ))])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "list_dependencies" => {
                let params: ListDependenciesParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let project_path = params.project_path.unwrap_or_else(|| ".".to_string());
                let include_dev = params.include_dev.unwrap_or(false);

                match self.list_dependencies_impl(&project_path, include_dev) {
                    Ok(json) => CallToolResult::success(vec![Content::text(json)]),
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "find_in_dependency" => {
                let params: FindInDependencyParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let kind_filter = params
                    .kind
                    .and_then(|k| SymbolKind::from_str(&k).map(|kind| vec![kind]));
                let options = SearchOptions {
                    limit: params.limit,
                    kind_filter,
                    language_filter: None,
                    file_filter: None,
                    name_filter: None,
                };

                match self
                    .index
                    .search_in_dependencies(&params.name, params.dependency.as_deref(), &options)
                {
                    Ok(results) => {
                        let json = serde_json::to_string_pretty(&results).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "get_dependency_source" => {
                let params: GetDependencySourceParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let context_lines = params.context_lines.unwrap_or(10);

                match self.get_dependency_source_impl(
                    &params.symbol_name,
                    params.dependency.as_deref(),
                    context_lines,
                ) {
                    Ok(source) => CallToolResult::success(vec![Content::text(source)]),
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "dependency_info" => {
                let params: DependencyInfoParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let project_path = params.project_path.unwrap_or_else(|| ".".to_string());

                match self.get_dependency_info_impl(&project_path, &params.name) {
                    Ok(json) => CallToolResult::success(vec![Content::text(json)]),
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "find_references" => {
                let params: FindReferencesParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let options = SearchOptions {
                    limit: params.limit,
                    file_filter: params.file_filter,
                    ..Default::default()
                };

                match self.index.find_references(&params.symbol_name, &options) {
                    Ok(refs) => {
                        let json = serde_json::to_string_pretty(&refs).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "find_callers" => {
                let params: FindCallersParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.find_callers(&params.function_name, params.depth) {
                    Ok(refs) => {
                        let json = serde_json::to_string_pretty(&refs).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "find_implementations" => {
                let params: FindImplementationsParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.find_implementations(&params.trait_name) {
                    Ok(symbols) => {
                        let json = serde_json::to_string_pretty(&symbols).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "get_symbol_members" => {
                let params: GetSymbolMembersParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.get_symbol_members(&params.type_name) {
                    Ok(symbols) => {
                        let json = serde_json::to_string_pretty(&symbols).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "get_file_imports" => {
                let params: GetFileImportsParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.get_file_imports(&params.file_path) {
                    Ok(imports) => {
                        let json = serde_json::to_string_pretty(&imports).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "get_file_importers" => {
                let params: GetFileImportersParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.get_file_importers(&params.file_path) {
                    Ok(importers) => {
                        let json = serde_json::to_string_pretty(&importers).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "search_by_pattern" => {
                let params: SearchByPatternParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.search_by_pattern_impl(&params.pattern, params.file_glob.as_deref(), params.limit) {
                    Ok(json) => CallToolResult::success(vec![Content::text(json)]),
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "batch_get_symbols" => {
                let params: BatchGetSymbolsParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let mut symbols = Vec::new();
                for id in params.symbol_ids {
                    if let Ok(Some(symbol)) = self.index.get_symbol(&id) {
                        symbols.push(symbol);
                    }
                }
                let json = serde_json::to_string_pretty(&symbols).unwrap_or_default();
                CallToolResult::success(vec![Content::text(json)])
            }
            "find_symbols_in_range" => {
                let params: FindSymbolsInRangeParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.get_file_symbols(&params.file_path) {
                    Ok(all_symbols) => {
                        let filtered: Vec<_> = all_symbols
                            .into_iter()
                            .filter(|s| {
                                s.location.start_line >= params.start_line
                                    && s.location.end_line <= params.end_line
                            })
                            .collect();
                        let json = serde_json::to_string_pretty(&filtered).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "list_modules" => {
                let params: ListModulesParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let workspace_path = params.workspace_path.unwrap_or_else(|| ".".to_string());

                match self.list_modules_impl(&workspace_path) {
                    Ok(json) => CallToolResult::success(vec![Content::text(json)]),
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "get_module_info" => {
                let params: GetModuleInfoParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let workspace_path = params.workspace_path.unwrap_or_else(|| ".".to_string());

                match self.get_module_info_impl(&workspace_path, &params.module_name) {
                    Ok(json) => CallToolResult::success(vec![Content::text(json)]),
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "find_module_dependencies" => {
                let params: FindModuleDependenciesParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let workspace_path = params.workspace_path.unwrap_or_else(|| ".".to_string());

                match self.find_module_dependencies_impl(&workspace_path, &params.module_name) {
                    Ok(json) => CallToolResult::success(vec![Content::text(json)]),
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "search_in_module" => {
                let params: SearchInModuleParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let workspace_path = params.workspace_path.unwrap_or_else(|| ".".to_string());

                match self.search_in_module_impl(
                    &workspace_path,
                    &params.module_name,
                    &params.query,
                    params.limit,
                ) {
                    Ok(json) => CallToolResult::success(vec![Content::text(json)]),
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "get_project_context" => {
                let params: GetProjectContextParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let project_path = params.project_path.unwrap_or_else(|| ".".to_string());

                match self.get_project_context_impl(&project_path) {
                    Ok(json) => CallToolResult::success(vec![Content::text(json)]),
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "get_architecture_summary" => {
                let params: GetArchitectureSummaryParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let project_path = params.project_path.unwrap_or_else(|| ".".to_string());

                match self.get_architecture_summary_impl(&project_path) {
                    Ok(json) => CallToolResult::success(vec![Content::text(json)]),
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "find_cross_language_refs" => {
                let params: FindCrossLanguageRefsParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.find_cross_language_refs_impl(
                    &params.symbol_name,
                    params.source_language.as_deref(),
                    params.target_language.as_deref(),
                ) {
                    Ok(json) => CallToolResult::success(vec![Content::text(json)]),
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "find_kotlin_extensions" => {
                let params: FindKotlinExtensionsParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.find_kotlin_extensions_impl(&params.java_type) {
                    Ok(json) => CallToolResult::success(vec![Content::text(json)]),
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "get_call_graph" => {
                let params: GetCallGraphParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let depth = params.depth.unwrap_or(3);
                match self.index.get_call_graph(&params.function, depth) {
                    Ok(graph) => {
                        let json = serde_json::to_string_pretty(&graph).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "find_callees" => {
                let params: FindCalleesParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.find_callees(&params.function) {
                    Ok(refs) => {
                        let json = serde_json::to_string_pretty(&refs).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "find_dead_code" => {
                match self.index.find_dead_code() {
                    Ok(report) => {
                        let json = serde_json::to_string_pretty(&report).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            "get_metrics" => {
                let params: GetMetricsParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let is_file = params.is_file.unwrap_or(false);
                let result = if is_file {
                    self.index.get_file_metrics(&params.target)
                } else {
                    self.index.get_function_metrics(&params.target)
                };

                match result {
                    Ok(metrics) => {
                        let json = serde_json::to_string_pretty(&metrics).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }
            _ => {
                return Err(McpError::invalid_params(
                    format!("Unknown tool: {}", request.name),
                    None,
                ));
            }
        };

        Ok(result)
    }
}
