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
use tracing::warn;

use crate::index::overlay::DocumentOverlay;
use crate::index::sqlite::SqliteIndex;
use crate::index::{CodeIndex, SearchOptions, SearchResult, SymbolKind};
use crate::mcp::consolidated::*;

/// Diversifies search results by limiting the number of results per directory.
/// Preserves ordering (results are kept in their original order, just capped per directory).
fn diversify_by_directory(results: Vec<SearchResult>, max_per_dir: usize) -> Vec<SearchResult> {
    use std::collections::HashMap;
    use std::path::Path;

    let mut dir_counts: HashMap<String, usize> = HashMap::new();
    let mut diversified = Vec::new();

    for result in results {
        let dir = Path::new(&result.symbol.location.file_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let count = dir_counts.entry(dir.clone()).or_insert(0);
        if *count < max_per_dir {
            *count += 1;
            diversified.push(result);
        }
    }

    diversified
}

#[derive(Clone)]
pub struct McpServer {
    index: Arc<SqliteIndex>,
    overlay: Arc<DocumentOverlay>,
    parse_cache: Arc<crate::indexer::ParseCache>,
    session_manager: Arc<code_indexer::session::SessionManager>,
    /// Optional write queue for serialized writes.
    /// When present, write operations go through this queue to prevent SQLITE_BUSY errors.
    write_queue: Option<crate::index::WriteQueueHandle>,
    indexing_progress: crate::indexer::IndexingProgress,
}

impl McpServer {
    pub fn new(index: Arc<SqliteIndex>) -> Self {
        Self {
            index,
            overlay: Arc::new(DocumentOverlay::new()),
            parse_cache: Arc::new(crate::indexer::ParseCache::new()),
            session_manager: Arc::new(code_indexer::session::SessionManager::new()),
            write_queue: None,
            indexing_progress: crate::indexer::IndexingProgress::new(),
        }
    }

    /// Creates a new McpServer with a write queue for serialized writes.
    /// This is recommended for production use to prevent SQLITE_BUSY errors
    /// when multiple concurrent write operations occur.
    pub fn with_write_queue(
        index: Arc<SqliteIndex>,
        write_queue: crate::index::WriteQueueHandle,
    ) -> Self {
        Self {
            index,
            overlay: Arc::new(DocumentOverlay::new()),
            parse_cache: Arc::new(crate::indexer::ParseCache::new()),
            session_manager: Arc::new(code_indexer::session::SessionManager::new()),
            write_queue: Some(write_queue),
            indexing_progress: crate::indexer::IndexingProgress::new(),
        }
    }

    #[allow(dead_code)]
    pub fn with_overlay(index: Arc<SqliteIndex>, overlay: Arc<DocumentOverlay>) -> Self {
        Self {
            index,
            overlay,
            parse_cache: Arc::new(crate::indexer::ParseCache::new()),
            session_manager: Arc::new(code_indexer::session::SessionManager::new()),
            write_queue: None,
            indexing_progress: crate::indexer::IndexingProgress::new(),
        }
    }

    /// Returns whether write queue is enabled.
    #[allow(dead_code)]
    pub fn has_write_queue(&self) -> bool {
        self.write_queue.is_some()
    }

    // === Write Queue Helper Methods ===
    // These methods use the write queue if available, otherwise fall back to direct writes.

    /// Removes files from the index. Uses write queue if available.
    async fn write_remove_files_batch(&self, file_paths: Vec<String>) -> crate::error::Result<()> {
        if let Some(ref wq) = self.write_queue {
            wq.remove_files_batch(file_paths).await
        } else {
            let file_refs: Vec<&str> = file_paths.iter().map(|s| s.as_str()).collect();
            self.index.remove_files_batch(&file_refs)
        }
    }

    /// Adds extraction results to the index. Uses write queue if available.
    async fn write_add_extraction_results(
        &self,
        results: Vec<crate::indexer::ExtractionResult>,
    ) -> crate::error::Result<usize> {
        if let Some(ref wq) = self.write_queue {
            wq.add_extraction_results(results).await
        } else {
            self.index.add_extraction_results_batch(results)
        }
    }

    /// Sets file content hash. Uses write queue if available.
    async fn write_set_file_content_hash(
        &self,
        file_path: &str,
        content_hash: &str,
    ) -> crate::error::Result<()> {
        if let Some(ref wq) = self.write_queue {
            wq.set_file_content_hash(file_path.to_string(), content_hash.to_string())
                .await
        } else {
            self.index.set_file_content_hash(file_path, content_hash)
        }
    }

    /// Adds file tags. Uses write queue if available.
    async fn write_add_file_tags(
        &self,
        file_path: &str,
        tags: &[crate::index::FileTag],
    ) -> crate::error::Result<()> {
        if let Some(ref wq) = self.write_queue {
            wq.add_file_tags(file_path.to_string(), tags.to_vec()).await
        } else {
            self.index.add_file_tags(file_path, tags)
        }
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

    #[allow(dead_code)]
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

    /// Load a code snippet from a file at the given location
    fn load_snippet(file_path: &str, start_line: u32, snippet_lines: usize) -> Option<String> {
        std::fs::read_to_string(file_path).ok().map(|content| {
            let lines: Vec<&str> = content.lines().collect();
            let start = (start_line.saturating_sub(1)) as usize;
            let end = (start + snippet_lines).min(lines.len());
            if start < lines.len() {
                lines[start..end].join("\n")
            } else {
                String::new()
            }
        })
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

    // === Tag Management ===

    fn handle_manage_tags(&self, params: ManageTagsParams) -> Result<ManageTagsResponse, McpError> {
        use code_indexer::indexer::{
            apply_tag_rules, preview_tag_rules, resolve_inferred_tags, RootSidecarData, TagRule,
            SIDECAR_FILENAME,
        };

        let project_path = params.path.as_deref().unwrap_or(".");
        let path = Path::new(project_path);
        let sidecar_path = path.join(SIDECAR_FILENAME);

        match params.action.as_str() {
            "add_rule" => {
                let pattern = params.pattern.ok_or_else(|| {
                    McpError::invalid_params("pattern is required for add_rule", None)
                })?;
                let tags = params.tags.ok_or_else(|| {
                    McpError::invalid_params("tags is required for add_rule", None)
                })?;
                let confidence = params.confidence.unwrap_or(0.7);

                // Load or create sidecar
                let mut data = if sidecar_path.exists() {
                    let content = fs::read_to_string(&sidecar_path)
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    RootSidecarData::parse(&content)
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?
                } else {
                    RootSidecarData::default()
                };

                // Check for existing rule
                if let Some(existing) = data.tag_rules.iter_mut().find(|r| r.pattern == pattern) {
                    for tag in &tags {
                        if !existing.tags.contains(tag) {
                            existing.tags.push(tag.clone());
                        }
                    }
                    existing.confidence = confidence;
                } else {
                    data.tag_rules.push(TagRule {
                        pattern: pattern.clone(),
                        tags: tags.clone(),
                        confidence,
                    });
                }

                // Save
                let content = serde_yaml::to_string(&data)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                fs::write(&sidecar_path, content)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                Ok(ManageTagsResponse {
                    success: true,
                    message: format!("Added rule: {} -> {:?}", pattern, tags),
                    ..Default::default()
                })
            }

            "remove_rule" => {
                let pattern = params.pattern.ok_or_else(|| {
                    McpError::invalid_params("pattern is required for remove_rule", None)
                })?;

                if !sidecar_path.exists() {
                    return Ok(ManageTagsResponse {
                        success: false,
                        message: "No .code-indexer.yml found".to_string(),
                        ..Default::default()
                    });
                }

                let content = fs::read_to_string(&sidecar_path)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                let mut data = RootSidecarData::parse(&content)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                let original_len = data.tag_rules.len();
                data.tag_rules.retain(|r| r.pattern != pattern);

                if data.tag_rules.len() < original_len {
                    let content = serde_yaml::to_string(&data)
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    fs::write(&sidecar_path, content)
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                    Ok(ManageTagsResponse {
                        success: true,
                        message: format!("Removed rule with pattern '{}'", pattern),
                        ..Default::default()
                    })
                } else {
                    Ok(ManageTagsResponse {
                        success: false,
                        message: format!("No rule found with pattern '{}'", pattern),
                        ..Default::default()
                    })
                }
            }

            "list_rules" => {
                if !sidecar_path.exists() {
                    return Ok(ManageTagsResponse {
                        success: true,
                        message: "No .code-indexer.yml found".to_string(),
                        rules: vec![],
                        ..Default::default()
                    });
                }

                let content = fs::read_to_string(&sidecar_path)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                let data = RootSidecarData::parse(&content)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                let rules: Vec<TagRuleInfo> = data
                    .tag_rules
                    .iter()
                    .map(|r| TagRuleInfo {
                        pattern: r.pattern.clone(),
                        tags: r.tags.clone(),
                        confidence: r.confidence,
                    })
                    .collect();

                Ok(ManageTagsResponse {
                    success: true,
                    message: format!("{} rules found", rules.len()),
                    rules,
                    ..Default::default()
                })
            }

            "preview" => {
                let file = params.file.ok_or_else(|| {
                    McpError::invalid_params("file is required for preview", None)
                })?;

                if !sidecar_path.exists() {
                    return Ok(ManageTagsResponse {
                        success: true,
                        message: "No tag rules defined".to_string(),
                        ..Default::default()
                    });
                }

                let content = fs::read_to_string(&sidecar_path)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                let data = RootSidecarData::parse(&content)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                // Get relative path
                let file_path = Path::new(&file);
                let relative_path = file_path
                    .strip_prefix(path)
                    .unwrap_or(file_path)
                    .to_string_lossy()
                    .to_string();

                let matches = preview_tag_rules(&relative_path, &data.tag_rules);

                let preview: Vec<TagPreviewResult> = matches
                    .iter()
                    .map(|m| TagPreviewResult {
                        pattern: m.pattern.clone(),
                        tags: m.tags.clone(),
                        confidence: m.confidence,
                    })
                    .collect();

                Ok(ManageTagsResponse {
                    success: true,
                    message: format!("{} rules matched", preview.len()),
                    preview,
                    ..Default::default()
                })
            }

            "apply" => {
                if !sidecar_path.exists() {
                    return Ok(ManageTagsResponse {
                        success: true,
                        message: "No tag rules to apply".to_string(),
                        ..Default::default()
                    });
                }

                let content = fs::read_to_string(&sidecar_path)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                let data = RootSidecarData::parse(&content)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                if data.tag_rules.is_empty() {
                    return Ok(ManageTagsResponse {
                        success: true,
                        message: "No tag rules defined".to_string(),
                        ..Default::default()
                    });
                }

                let tag_dict = self.index.get_tag_dictionary().unwrap_or_default();
                let files = self.index.get_indexed_files()
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                let mut applied_count = 0;
                let mut warnings = Vec::new();

                for file_path in &files {
                    let relative_path = Path::new(file_path)
                        .strip_prefix(path)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| file_path.clone());

                    let inferred = apply_tag_rules(&relative_path, &data.tag_rules);
                    if !inferred.is_empty() {
                        let result = resolve_inferred_tags(file_path, &inferred, &tag_dict);

                        if !result.tags.is_empty() {
                            if let Err(e) = self.index.add_file_tags(file_path, &result.tags) {
                                warnings.push(format!("Failed to add tags to {}: {}", file_path, e));
                            } else {
                                applied_count += result.tags.len();
                            }
                        }

                        for unknown in result.unknown_tags {
                            warnings.push(format!("Unknown tag '{}' for file {}", unknown, file_path));
                        }
                    }
                }

                Ok(ManageTagsResponse {
                    success: true,
                    message: format!("Applied {} tags to files", applied_count),
                    warnings,
                    ..Default::default()
                })
            }

            "stats" => {
                let stats = self.index.get_tag_stats()
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                let tag_stats: Vec<TagStatInfo> = stats
                    .into_iter()
                    .map(|(category, tag, count)| TagStatInfo { category, tag, count })
                    .collect();

                let total: usize = tag_stats.iter().map(|s| s.count).sum();

                Ok(ManageTagsResponse {
                    success: true,
                    message: format!("{} total tag assignments", total),
                    stats: tag_stats,
                    ..Default::default()
                })
            }

            _ => Err(McpError::invalid_params(
                format!("Unknown action: {}. Valid actions: add_rule, remove_rule, list_rules, preview, apply, stats", params.action),
                None,
            )),
        }
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

    // === Summary-First Contract Implementation ===

    fn get_context_bundle_impl(
        &self,
        params: GetContextBundleParams,
    ) -> crate::error::Result<crate::index::ResponseEnvelope<ContextBundle>> {
        use crate::index::{
            CodeIndex, CountsInfo, NextAction, OutputFormat, PaginationCursor, ResponseEnvelope,
            SearchOptions,
        };
        use std::collections::HashMap;

        let input = params.input.unwrap_or_default();
        let budget = params.budget.unwrap_or_default();
        let format = params
            .format
            .as_deref()
            .and_then(OutputFormat::from_str)
            .unwrap_or(OutputFormat::Minimal);

        let max_items = budget.max_items.unwrap_or(20);
        let sample_k = budget.sample_k.unwrap_or(5);
        let include_snippets = budget.include_snippets.unwrap_or(false);
        let snippet_lines = budget.snippet_lines.unwrap_or(3);

        let mut symbol_cards: Vec<SymbolCard> = Vec::new();
        let mut top_usages: Vec<UsageRef> = Vec::new();
        let mut call_neighborhood: Option<CallNeighborhood> = None;
        let mut imports_relevant: Vec<RelevantImport> = Vec::new();
        let mut next_actions: Vec<NextAction> = Vec::new();

        // Search by query if provided
        if let Some(ref query) = input.query {
            let options = SearchOptions {
                limit: Some(max_items),
                current_file: input.file.clone(),
                use_advanced_ranking: Some(true),
                ..Default::default()
            };

            let results = self.index.search(query, &options)?;

            // Build symbol cards
            for (rank, result) in results.iter().enumerate() {
                let symbol = &result.symbol;
                let stable_id = symbol.compute_stable_id(None);

                let card = SymbolCard {
                    id: stable_id,
                    fqdn: symbol.fqdn.clone(),
                    kind: symbol.kind.as_str().to_string(),
                    sig: symbol.signature.clone(),
                    loc: format!(
                        "{}:{}",
                        symbol.location.file_path, symbol.location.start_line
                    ),
                    rank: (rank + 1) as u32,
                    snippet: if include_snippets {
                        Self::load_snippet(
                            &symbol.location.file_path,
                            symbol.location.start_line,
                            snippet_lines,
                        )
                    } else {
                        None
                    },
                };
                symbol_cards.push(card);
            }

            // Get top usages (diversified: 1-2 per file)
            if !symbol_cards.is_empty() {
                let first_symbol_name = &results[0].symbol.name;
                if let Ok(refs) = self
                    .index
                    .find_references(first_symbol_name, &SearchOptions::default())
                {
                    let mut files_seen: HashMap<String, usize> = HashMap::new();
                    for r in refs.iter().take(10) {
                        let count = files_seen.entry(r.file_path.clone()).or_insert(0);
                        if *count < 2 {
                            top_usages.push(UsageRef {
                                file: r.file_path.clone(),
                                line: r.line,
                                context: None,
                                kind: r.kind.as_str().to_string(),
                            });
                            *count += 1;
                        }
                    }
                }
            }

            // Suggest next actions if more results available
            if results.len() == max_items {
                let cursor = PaginationCursor::from_offset(max_items);
                next_actions.push(
                    NextAction::new(
                        "get_context_bundle",
                        serde_json::json!({
                            "input": { "query": query },
                            "cursor": cursor.encode()
                        }),
                    )
                    .with_hint("Load more results"),
                );
            }
        }

        // Lookup by symbol IDs if provided
        if let Some(ref ids) = input.symbol_ids {
            for sid in ids.iter().take(max_items) {
                // Try stable_id first, then regular id
                let symbol = if sid.starts_with("sid:") {
                    self.index.get_symbol_by_stable_id(sid)?
                } else {
                    self.index.get_symbol(sid)?
                };

                if let Some(symbol) = symbol {
                    let stable_id = symbol.compute_stable_id(None);
                    let card = SymbolCard {
                        id: stable_id,
                        fqdn: symbol.fqdn.clone(),
                        kind: symbol.kind.as_str().to_string(),
                        sig: symbol.signature.clone(),
                        loc: format!(
                            "{}:{}",
                            symbol.location.file_path, symbol.location.start_line
                        ),
                        rank: (symbol_cards.len() + 1) as u32,
                        snippet: if include_snippets {
                            Self::load_snippet(
                                &symbol.location.file_path,
                                symbol.location.start_line,
                                snippet_lines,
                            )
                        } else {
                            None
                        },
                    };
                    symbol_cards.push(card);
                }
            }
        }

        // Get call neighborhood for first symbol
        if !symbol_cards.is_empty() {
            if let Some(first_card) = symbol_cards.first() {
                // Find symbol by ID to get its name
                let symbol_name = if let Some(ref query) = input.query {
                    query.clone()
                } else {
                    first_card.id.clone()
                };

                let mut callers = Vec::new();
                let mut callees = Vec::new();

                // Get callers
                if let Ok(refs) = self.index.find_callers(&symbol_name, Some(1)) {
                    for r in refs.iter().take(5) {
                        callers.push(CallRef {
                            name: r.symbol_name.clone(),
                            id: r.symbol_id.clone(),
                            loc: format!("{}:{}", r.file_path, r.line),
                            confidence: "certain".to_string(),
                        });
                    }
                }

                // Get callees
                if let Ok(refs) = self.index.find_callees(&symbol_name) {
                    for r in refs.iter().take(5) {
                        callees.push(CallRef {
                            name: r.symbol_name.clone(),
                            id: r.symbol_id.clone(),
                            loc: format!("{}:{}", r.file_path, r.line),
                            confidence: "certain".to_string(),
                        });
                    }
                }

                if !callers.is_empty() || !callees.is_empty() {
                    call_neighborhood = Some(CallNeighborhood { callers, callees });
                }
            }
        }

        // Get relevant imports for current file
        if let Some(ref file) = input.file {
            if let Ok(imports) = self.index.get_file_imports(file) {
                for imp in imports.iter().take(10) {
                    imports_relevant.push(RelevantImport {
                        path: imp.imported_path.clone().unwrap_or_default(),
                        symbol: imp.imported_symbol.clone(),
                        from_file: imp.file_path.clone(),
                    });
                }
            }
        }

        // Build the bundle
        let bundle = ContextBundle {
            symbol_cards: symbol_cards.clone(),
            top_usages,
            call_neighborhood,
            imports_relevant,
        };

        // Build response envelope
        let total = symbol_cards.len();
        let truncated = total >= max_items;

        let envelope = if truncated {
            let sample: Vec<ContextBundle> = vec![bundle.clone()];
            let counts = CountsInfo::new(total, sample_k.min(total));
            ResponseEnvelope::truncated(sample, counts, None)
        } else {
            ResponseEnvelope::with_items(vec![bundle], format)
        };

        Ok(envelope.with_next(next_actions))
    }
}

// === Helper Functions for Envelope Support ===

/// Wrap a value in ResponseEnvelope for backward compatibility
///
/// Usage: `wrap_in_envelope(results, envelope_requested)`
/// If `envelope_requested` is true, wraps in envelope; otherwise returns raw JSON
#[allow(dead_code)]
pub fn wrap_in_envelope<T: serde::Serialize + Clone>(
    items: Vec<T>,
    use_envelope: bool,
    format: crate::index::OutputFormat,
) -> String {
    if use_envelope {
        let envelope = crate::index::ResponseEnvelope::with_items(items, format);
        serde_json::to_string_pretty(&envelope).unwrap_or_default()
    } else {
        serde_json::to_string_pretty(&items).unwrap_or_default()
    }
}

/// Wrap a single value in ResponseEnvelope
#[allow(dead_code)]
pub fn wrap_single_in_envelope<T: serde::Serialize + Clone>(
    item: T,
    use_envelope: bool,
    format: crate::index::OutputFormat,
) -> String {
    if use_envelope {
        let envelope = crate::index::ResponseEnvelope::with_items(vec![item], format);
        serde_json::to_string_pretty(&envelope).unwrap_or_default()
    } else {
        serde_json::to_string_pretty(&item).unwrap_or_default()
    }
}

/// Extract envelope parameter from request arguments
#[allow(dead_code)]
pub fn extract_envelope_param(args: &Option<serde_json::Map<String, serde_json::Value>>) -> bool {
    args.as_ref()
        .and_then(|m| m.get("envelope"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Enforces max_bytes budget on serialized output.
/// Returns (truncated_output, was_truncated) tuple.
/// If max_bytes is None, returns the full output without truncation.
#[allow(dead_code)]
pub fn enforce_max_bytes(output: &str, max_bytes: Option<usize>) -> (String, bool) {
    match max_bytes {
        Some(max) if output.len() > max => {
            // Find a safe truncation point (at a character boundary)
            let truncated = if max >= 3 {
                let safe_max = output.floor_char_boundary(max.saturating_sub(3));
                format!("{}...", &output[..safe_max])
            } else {
                "...".to_string()
            };
            (truncated, true)
        }
        _ => (output.to_string(), false),
    }
}

/// Enforces budget on a list of items by serializing and truncating if needed.
/// Returns (items, total_count, was_truncated).
#[allow(dead_code)]
pub fn enforce_budget_on_items<T: serde::Serialize + Clone>(
    items: Vec<T>,
    max_items: Option<usize>,
    max_bytes: Option<usize>,
) -> (Vec<T>, usize, bool) {
    let total_count = items.len();

    // First apply max_items limit
    let limited_items: Vec<T> = match max_items {
        Some(max) if items.len() > max => items.into_iter().take(max).collect(),
        _ => items,
    };

    let was_truncated_by_items = max_items.map_or(false, |max| total_count > max);

    // Then check if serialized size exceeds max_bytes
    if let Some(max_bytes) = max_bytes {
        let mut result = limited_items.clone();
        let limited_len = limited_items.len();
        loop {
            let json = serde_json::to_string(&result).unwrap_or_default();
            if json.len() <= max_bytes || result.is_empty() {
                let was_truncated = was_truncated_by_items || result.len() < limited_len;
                return (result, total_count, was_truncated);
            }
            // Remove last item and try again
            result.pop();
        }
    }

    (limited_items, total_count, was_truncated_by_items)
}

/// Redacts sensitive information from code snippets.
/// Replaces patterns that look like secrets with [REDACTED].
#[allow(dead_code)]
pub fn redact_secrets(content: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;

    // Static patterns compiled once
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();

    let patterns = PATTERNS.get_or_init(|| {
        vec![
            // API keys (various formats)
            Regex::new(r#"(?i)(api[_-]?key|apikey)\s*[=:]\s*["']?[A-Za-z0-9_\-]{20,}["']?"#).unwrap(),
            // Bearer tokens
            Regex::new(r#"(?i)bearer\s+[A-Za-z0-9_\-\.]{20,}"#).unwrap(),
            // AWS keys
            Regex::new(r#"(?i)(aws[_-]?access[_-]?key[_-]?id|aws[_-]?secret[_-]?access[_-]?key)\s*[=:]\s*["']?[A-Za-z0-9/+=]{20,}["']?"#).unwrap(),
            // Private keys / passwords in assignments
            Regex::new(r#"(?i)(private[_-]?key|secret[_-]?key|password|passwd|pwd)\s*[=:]\s*["'][^"']{8,}["']"#).unwrap(),
            // Generic secrets in environment-style
            Regex::new(r#"(?i)(secret|token|credential|auth)\s*[=:]\s*["']?[A-Za-z0-9_\-]{16,}["']?"#).unwrap(),
            // JWT tokens
            Regex::new(r#"eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}"#).unwrap(),
            // GitHub tokens
            Regex::new(r#"gh[pousr]_[A-Za-z0-9]{36,}"#).unwrap(),
            // Connection strings with passwords
            Regex::new(r#"(?i)(postgres|mysql|mongodb|redis)://[^:]+:[^@]+@"#).unwrap(),
        ]
    });

    let mut result = content.to_string();
    for pattern in patterns.iter() {
        result = pattern.replace_all(&result, "[REDACTED]").to_string();
    }
    result
}

fn schema_for<T: JsonSchema>() -> Arc<serde_json::Map<String, serde_json::Value>> {
    let schema = schemars::schema_for!(T);
    let value = serde_json::to_value(&schema).expect("Failed to serialize schema");
    match value {
        serde_json::Value::Object(map) => Arc::new(map),
        _ => Arc::new(serde_json::Map::new()),
    }
}

// === Legacy params for backward compatibility (deprecated) ===

#[allow(dead_code)]
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
    /// Output format: full, compact, minimal
    #[serde(default)]
    pub format: Option<String>,
    /// Enable fuzzy search for typo tolerance
    #[serde(default)]
    pub fuzzy: Option<bool>,
    /// Fuzzy search threshold (0.0-1.0)
    #[serde(default)]
    pub fuzzy_threshold: Option<f64>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindDefinitionParams {
    /// Symbol name
    pub name: String,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetFileStructureParams {
    /// File path
    pub file_path: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetSymbolParams {
    /// ID
    pub id: String,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetFileImportsParams {
    /// File path
    pub file_path: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetFileImportersParams {
    /// File path
    pub file_path: String,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BatchGetSymbolsParams {
    /// IDs
    pub symbol_ids: Vec<String>,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetCallGraphParams {
    /// Entry point function name
    pub function: String,
    /// Maximum depth of the call graph (default: 3)
    #[serde(default)]
    pub depth: Option<u32>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindCalleesParams {
    /// Function name
    pub function: String,
}

#[allow(dead_code)]
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct FindDeadCodeParams {
    /// Optional path filter
    #[serde(default)]
    pub path_filter: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetMetricsParams {
    /// Function name or file path
    pub target: String,
    /// Whether the target is a file path (default: false, meaning it's a function name)
    #[serde(default)]
    pub is_file: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetChangedSymbolsParams {
    /// Git reference to compare against (default: HEAD)
    #[serde(default)]
    pub base: Option<String>,
    /// Include staged changes
    #[serde(default)]
    pub include_staged: Option<bool>,
    /// Include unstaged changes
    #[serde(default)]
    pub include_unstaged: Option<bool>,
    /// Output format: full, compact, minimal
    #[serde(default)]
    pub format: Option<String>,
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
        // === 12 Consolidated MCP Tools ===
        let tools = vec![
            // 1. index_workspace - Index workspace with configuration
            Tool {
                name: "index_workspace".into(),
                title: Some("Index Workspace".to_string()),
                description: Some("Index a workspace with optional configuration. Supports file watching and dependency indexing.".into()),
                input_schema: schema_for::<IndexWorkspaceParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 2. update_files - Update virtual documents (for unsaved changes)
            Tool {
                name: "update_files".into(),
                title: Some("Update Files".to_string()),
                description: Some("Update virtual documents for unsaved file changes. Supports versioning for conflict detection.".into()),
                input_schema: schema_for::<UpdateFilesParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 3. list_symbols - List symbols with filters (replaces list_functions, list_types)
            Tool {
                name: "list_symbols".into(),
                title: Some("List Symbols".to_string()),
                description: Some("List symbols with filters. Use kind='function' or kind='type' to filter. Supports language, file pattern, and output format options.".into()),
                input_schema: schema_for::<ListSymbolsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 4. search_symbols - Search symbols with fuzzy matching (replaces search_symbol, search_by_pattern, search_in_module)
            Tool {
                name: "search_symbols".into(),
                title: Some("Search Symbols".to_string()),
                description: Some("Search symbols with optional fuzzy matching, regex patterns, and module filtering. Supports ranking and multiple output formats.".into()),
                input_schema: schema_for::<SearchSymbolsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 5. get_symbol - Get symbol by ID or position (replaces get_symbol, batch_get_symbols)
            Tool {
                name: "get_symbol".into(),
                title: Some("Get Symbol".to_string()),
                description: Some("Get a symbol by ID, batch of IDs, or by file position (file + line + column).".into()),
                input_schema: schema_for::<ConsolidatedGetSymbolParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 6. find_definitions - Find symbol definitions (with dependency support)
            Tool {
                name: "find_definitions".into(),
                title: Some("Find Definitions".to_string()),
                description: Some("Find symbol definitions. Can search in project and dependencies.".into()),
                input_schema: schema_for::<FindDefinitionsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 7. find_references - Find symbol references (replaces find_references, find_callers, get_file_importers)
            Tool {
                name: "find_references".into(),
                title: Some("Find References".to_string()),
                description: Some("Find symbol references. Includes callers, importers, and filters by reference kind.".into()),
                input_schema: schema_for::<ConsolidatedFindReferencesParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 8. analyze_call_graph - Call graph analysis (replaces get_call_graph, find_callees)
            Tool {
                name: "analyze_call_graph".into(),
                title: Some("Analyze Call Graph".to_string()),
                description: Some("Analyze call graph from entry point. Supports bidirectional traversal and confidence filtering.".into()),
                input_schema: schema_for::<AnalyzeCallGraphParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 9. get_file_outline - Get file structure (replaces get_file_structure, find_symbols_in_range)
            Tool {
                name: "get_file_outline".into(),
                title: Some("Get File Outline".to_string()),
                description: Some("Get file structure/outline. Supports line range selection and nested scopes.".into()),
                input_schema: schema_for::<GetFileOutlineParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 10. get_imports - Get file imports
            Tool {
                name: "get_imports".into(),
                title: Some("Get Imports".to_string()),
                description: Some("Get imports for a file. Can resolve imports to their definitions.".into()),
                input_schema: schema_for::<GetImportsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 11. get_diagnostics - Get diagnostics (replaces find_dead_code)
            Tool {
                name: "get_diagnostics".into(),
                title: Some("Get Diagnostics".to_string()),
                description: Some("Get code diagnostics including dead code detection and metrics.".into()),
                input_schema: schema_for::<GetDiagnosticsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 12. get_stats - Get index statistics
            Tool {
                name: "get_stats".into(),
                title: Some("Get Stats".to_string()),
                description: Some("Get index statistics with optional workspace, dependency, and architecture details.".into()),
                input_schema: schema_for::<GetStatsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 13. get_context_bundle - Summary-first AI-agent entry point
            Tool {
                name: "get_context_bundle".into(),
                title: Some("Get Context Bundle".to_string()),
                description: Some(
                    "Primary AI-agent entry point. Returns symbol cards, top usages, call neighborhood, \
                     and relevant imports in a single call. Supports budget constraints for token efficiency. \
                     Use this instead of multiple search/find calls for better context understanding."
                        .into(),
                ),
                input_schema: schema_for::<GetContextBundleParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 14. get_snippet - Retrieve code snippets with budget control
            Tool {
                name: "get_snippet".into(),
                title: Some("Get Snippet".to_string()),
                description: Some(
                    "Retrieve code snippets by stable_id or file:line reference. \
                     Supports context lines, max line limits, and scope expansion. \
                     Use this as the single code-returning tool with budget control."
                        .into(),
                ),
                input_schema: schema_for::<GetSnippetParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 15. get_doc_section - Get documentation section from README/docs
            Tool {
                name: "get_doc_section".into(),
                title: Some("Get Doc Section".to_string()),
                description: Some(
                    "Extract sections from documentation files (README, CONTRIBUTING, etc.). \
                     Returns headings, code blocks, and section content. Use for installation \
                     instructions, usage examples, and API documentation."
                        .into(),
                ),
                input_schema: schema_for::<GetDocSectionParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 16. get_project_commands - Get run/build/test commands from config files
            Tool {
                name: "get_project_commands".into(),
                title: Some("Get Project Commands".to_string()),
                description: Some(
                    "Extract run, build, and test commands from project configuration files \
                     (package.json, Cargo.toml, Makefile, etc.). Useful for understanding \
                     how to build and run the project."
                        .into(),
                ),
                input_schema: schema_for::<GetProjectCommandsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 17. get_project_compass - Macro-level project overview
            Tool {
                name: "get_project_compass".into(),
                title: Some("Get Project Compass".to_string()),
                description: Some(
                    "Get a macro-level overview of the project including languages, frameworks, \
                     entry points, module hierarchy, and available commands. This is the primary \
                     starting point for understanding an unfamiliar codebase."
                        .into(),
                ),
                input_schema: schema_for::<GetProjectCompassParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 18. expand_project_node - Drill-down into modules
            Tool {
                name: "expand_project_node".into(),
                title: Some("Expand Project Node".to_string()),
                description: Some(
                    "Drill down into a specific module or directory from the project compass. \
                     Returns child nodes, files, and optionally symbols within the node."
                        .into(),
                ),
                input_schema: schema_for::<ExpandProjectNodeParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 19. get_compass - Task-oriented diversified search
            Tool {
                name: "get_compass".into(),
                title: Some("Get Compass".to_string()),
                description: Some(
                    "Task-oriented search that returns diversified results across symbols, files, \
                     modules, docs, and commands. Use for feature location and understanding \
                     where specific functionality lives in the codebase."
                        .into(),
                ),
                input_schema: schema_for::<GetCompassQueryParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 20. open_session - Open a session for token optimization
            Tool {
                name: "open_session".into(),
                title: Some("Open Session".to_string()),
                description: Some(
                    "Open a session for dictionary-based token optimization. Returns a session ID \
                     and initial dictionary mapping long strings to short IDs."
                        .into(),
                ),
                input_schema: schema_for::<OpenSessionParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 21. close_session - Close a session
            Tool {
                name: "close_session".into(),
                title: Some("Close Session".to_string()),
                description: Some(
                    "Close an open session and release its resources."
                        .into(),
                ),
                input_schema: schema_for::<CloseSessionParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 22. manage_tags - Manage tag inference rules
            Tool {
                name: "manage_tags".into(),
                title: Some("Manage Tags".to_string()),
                description: Some(
                    "Manage tag inference rules in .code-indexer.yml. Actions: add_rule, remove_rule, \
                     list_rules, preview (what tags would be inferred for a file), apply (apply rules to index), \
                     stats (show tag statistics). Rules use glob patterns to match file paths."
                        .into(),
                ),
                input_schema: schema_for::<ManageTagsParams>(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // 23. get_indexing_status - Check indexing progress
            Tool {
                name: "get_indexing_status".into(),
                title: Some("Get Indexing Status".to_string()),
                description: Some(
                    "Check current indexing progress. Returns files processed/total, symbols extracted, \
                     errors, progress percentage, elapsed time, and ETA. Use to poll indexing status \
                     after calling index_workspace."
                        .into(),
                ),
                input_schema: schema_for::<GetIndexingStatusParams>(),
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
        // Map deprecated tool names to new consolidated tools
        let tool_name = match request.name.as_ref() {
            // Deprecated aliases → new tools (with warning)
            "search_symbol" | "search_by_pattern" | "search_in_module" => {
                warn!(
                    "Tool '{}' is deprecated, use 'search_symbols' instead",
                    request.name
                );
                "search_symbols"
            }
            "list_functions" | "list_types" => {
                warn!(
                    "Tool '{}' is deprecated, use 'list_symbols' instead",
                    request.name
                );
                "list_symbols"
            }
            "find_definition" | "find_in_dependency" => {
                warn!(
                    "Tool '{}' is deprecated, use 'find_definitions' instead",
                    request.name
                );
                "find_definitions"
            }
            "find_callers" | "get_file_importers" => {
                warn!(
                    "Tool '{}' is deprecated, use 'find_references' instead",
                    request.name
                );
                "find_references"
            }
            "get_call_graph" | "find_callees" => {
                warn!(
                    "Tool '{}' is deprecated, use 'analyze_call_graph' instead",
                    request.name
                );
                "analyze_call_graph"
            }
            "get_file_structure" | "find_symbols_in_range" => {
                warn!(
                    "Tool '{}' is deprecated, use 'get_file_outline' instead",
                    request.name
                );
                "get_file_outline"
            }
            "get_file_imports" => {
                warn!(
                    "Tool '{}' is deprecated, use 'get_imports' instead",
                    request.name
                );
                "get_imports"
            }
            "find_dead_code" | "get_metrics" => {
                warn!(
                    "Tool '{}' is deprecated, use 'get_diagnostics' instead",
                    request.name
                );
                "get_diagnostics"
            }
            "index_stats" => {
                warn!(
                    "Tool '{}' is deprecated, use 'get_stats' instead",
                    request.name
                );
                "get_stats"
            }
            "batch_get_symbols" => {
                warn!(
                    "Tool '{}' is deprecated, use 'get_symbol' with 'ids' parameter instead",
                    request.name
                );
                "get_symbol"
            }
            other => other,
        };

        let result = match tool_name {
            // === 1. index_workspace ===
            "index_workspace" => {
                use crate::index::sqlite::{IndexedFileRecord, SqliteIndex};
                use crate::indexer::{FileWalker, Parser, SymbolExtractor};
                use crate::languages::LanguageRegistry;
                use rayon::prelude::*;
                use std::collections::HashSet;

                let params: IndexWorkspaceParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let workspace_path = params.path.unwrap_or_else(|| ".".to_string());
                let path = std::path::Path::new(&workspace_path);

                // Walk the workspace to find supported files
                let registry = LanguageRegistry::new();
                let walker = FileWalker::new(registry);

                let files = walker.walk(path).map_err(|e| {
                    McpError::internal_error(format!("Failed to walk workspace: {}", e), None)
                })?;

                let files_count = files.len();
                let current_files: HashSet<String> = files
                    .iter()
                    .map(|file| file.to_string_lossy().to_string())
                    .collect();
                let tracked_files = self.index.get_tracked_files().map_err(|e| {
                    McpError::internal_error(format!("Failed to list tracked files: {}", e), None)
                })?;
                let stale_files: Vec<String> = tracked_files
                    .into_iter()
                    .filter(|tracked| !current_files.contains(tracked))
                    .collect();
                if !stale_files.is_empty() {
                    let stale_refs: Vec<&str> = stale_files.iter().map(|s| s.as_str()).collect();
                    self.index.remove_files_batch(&stale_refs).map_err(|e| {
                        McpError::internal_error(
                            format!("Failed to remove stale files: {}", e),
                            None,
                        )
                    })?;
                }

                // Incremental indexing: filter out files that haven't changed
                let files_to_index: Vec<_> = files
                    .iter()
                    .filter(|file| {
                        // Read file content and compute hash
                        if let Ok(content) = std::fs::read_to_string(file) {
                            let hash = SqliteIndex::compute_content_hash(&content);
                            if let Ok(needs_reindex) = self
                                .index
                                .file_needs_reindex(&file.to_string_lossy(), &hash)
                            {
                                if !needs_reindex {
                                    return false; // Skip unchanged file
                                }
                            }
                        }
                        true
                    })
                    .collect();

                let files_skipped = files_count - files_to_index.len();

                // Start progress tracking
                self.indexing_progress.start(files_to_index.len());
                let progress_ref = &self.indexing_progress;

                // Parallel parsing and extraction using rayon
                let parse_cache = self.parse_cache.clone();
                let results: Vec<(IndexedFileRecord, crate::indexer::ExtractionResult)> =
                    files_to_index
                        .into_par_iter()
                        .map_init(
                            || (Parser::new(LanguageRegistry::new()), SymbolExtractor::new()),
                            |(parser, extractor), file| {
                                // Read content and compute hash
                                let content = std::fs::read_to_string(file).ok()?;
                                let hash = SqliteIndex::compute_content_hash(&content);

                                match parse_cache.parse_source_cached(file, &content, parser) {
                                    Ok(parsed) => match extractor.extract_all(&parsed, file) {
                                        Ok(result) => {
                                            let symbol_count = result.symbols.len();
                                            progress_ref.inc(symbol_count);
                                            Some((
                                                IndexedFileRecord {
                                                    path: file.to_string_lossy().to_string(),
                                                    language: parsed.language.clone(),
                                                    symbol_count,
                                                    content_hash: hash,
                                                },
                                                result,
                                            ))
                                        }
                                        Err(_) => {
                                            progress_ref.inc_error();
                                            None
                                        }
                                    },
                                    Err(_) => {
                                        progress_ref.inc_error();
                                        parse_cache.invalidate(file);
                                        None
                                    }
                                }
                            },
                        )
                        .filter_map(|r| r)
                        .collect();

                let files_updated = results.len();

                // Remove old data for files that will be updated (batch operation)
                let file_paths: Vec<String> = results.iter().map(|(f, _)| f.path.clone()).collect();
                let file_refs: Vec<&str> = file_paths.iter().map(|s| s.as_str()).collect();
                let _ = self.index.remove_files_batch(&file_refs);

                // Split records and extracted symbols for storage.
                let mut file_records = Vec::with_capacity(results.len());
                let mut extraction_results = Vec::with_capacity(results.len());
                for (file_record, extraction_result) in results {
                    file_records.push(file_record);
                    extraction_results.push(extraction_result);
                }

                // Batch insert all results
                let total_symbols = self
                    .index
                    .add_extraction_results_batch(extraction_results)
                    .map_err(|e| {
                        McpError::internal_error(format!("Failed to store results: {}", e), None)
                    })?;

                // Persist file metadata/content hashes for incremental updates.
                self.index
                    .upsert_file_records_batch(&file_records)
                    .map_err(|e| {
                        McpError::internal_error(
                            format!("Failed to update file records: {}", e),
                            None,
                        )
                    })?;

                self.indexing_progress.finish();

                let output = serde_json::json!({
                    "status": "indexed",
                    "path": workspace_path,
                    "files_found": files_count,
                    "files_updated": files_updated,
                    "files_skipped": files_skipped,
                    "symbols_indexed": total_symbols,
                    "incremental": true,
                    "watch": params.watch.unwrap_or(false),
                    "include_deps": params.include_deps.unwrap_or(false),
                    "progress": "completed",
                });
                CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&output).unwrap_or_default(),
                )])
            }

            // === 2. update_files ===
            "update_files" => {
                use crate::indexer::{Parser, SymbolExtractor};
                use crate::languages::LanguageRegistry;

                let params: UpdateFilesParams = serde_json::from_value(serde_json::Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let registry = LanguageRegistry::new();
                let parser = Parser::new(registry);
                let extractor = SymbolExtractor::new();

                let mut updated_files = Vec::new();
                let mut warnings = Vec::new();
                let mut symbol_counts = Vec::new();

                for file_update in params.files {
                    let path = &file_update.path;
                    let content = &file_update.content;
                    let version = file_update.version.unwrap_or(1);

                    // Check version conflict
                    if let Some(current_version) = self.overlay.get_version(path) {
                        if version <= current_version {
                            warnings.push(serde_json::json!({
                                "file": path,
                                "warning": "version_conflict",
                                "current_version": current_version,
                                "provided_version": version
                            }));
                            continue;
                        }
                    }

                    // Update overlay with new content
                    self.overlay.update(path, content, version);

                    // Parse and extract symbols from content using incremental ParseCache.
                    let file_path = std::path::Path::new(path);
                    match self
                        .parse_cache
                        .parse_source_cached(file_path, content, &parser)
                    {
                        Ok(parsed) => match extractor.extract_all(&parsed, file_path) {
                            Ok(result) => {
                                let symbols_count = result.symbols.len();
                                self.overlay.set_symbols(path, result.symbols);
                                symbol_counts.push(serde_json::json!({
                                    "file": path,
                                    "symbols": symbols_count
                                }));
                            }
                            Err(e) => {
                                warnings.push(serde_json::json!({
                                    "file": path,
                                    "warning": "extraction_failed",
                                    "error": e.to_string()
                                }));
                            }
                        },
                        Err(crate::error::IndexerError::UnsupportedLanguage(_)) => {
                            warnings.push(serde_json::json!({
                                "file": path,
                                "warning": "unsupported_language"
                            }));
                        }
                        Err(e) => {
                            warnings.push(serde_json::json!({
                                "file": path,
                                "warning": "parse_failed",
                                "error": e.to_string()
                            }));
                            self.parse_cache.invalidate(file_path);
                        }
                    }

                    updated_files.push(path.clone());
                }

                let mut output = serde_json::json!({
                    "status": "updated",
                    "files": updated_files,
                    "symbol_counts": symbol_counts,
                });

                if !warnings.is_empty() {
                    output["warnings"] = serde_json::json!(warnings);
                }

                CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&output).unwrap_or_default(),
                )])
            }

            // === 3. list_symbols ===
            "list_symbols" => {
                use crate::index::{CompactSymbol, OutputFormat};

                let params: ListSymbolsParams = serde_json::from_value(serde_json::Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let kind_str = params.kind.as_deref().unwrap_or("all");
                let output_format = params
                    .format
                    .as_deref()
                    .and_then(OutputFormat::from_str)
                    .unwrap_or(OutputFormat::Full);

                let options = SearchOptions {
                    limit: params.limit,
                    language_filter: params.language.map(|l| vec![l]),
                    file_filter: params.file,
                    name_filter: params.pattern,
                    output_format: Some(output_format),
                    ..Default::default()
                };

                let symbols_result = match kind_str {
                    "function" | "functions" => self.index.list_functions(&options),
                    "type" | "types" => self.index.list_types(&options),
                    _ => {
                        // Get both functions and types
                        let mut all = self.index.list_functions(&options).unwrap_or_default();
                        all.extend(self.index.list_types(&options).unwrap_or_default());
                        if let Some(limit) = params.limit {
                            all.truncate(limit);
                        }
                        Ok(all)
                    }
                };

                match symbols_result {
                    Ok(symbols) => {
                        let output = match output_format {
                            OutputFormat::Full => {
                                serde_json::to_string_pretty(&symbols).unwrap_or_default()
                            }
                            OutputFormat::Compact => {
                                let compact: Vec<CompactSymbol> = symbols
                                    .iter()
                                    .map(|s| CompactSymbol::from_symbol(s, None))
                                    .collect();
                                serde_json::to_string(&compact).unwrap_or_default()
                            }
                            OutputFormat::Minimal => symbols
                                .iter()
                                .map(|s| CompactSymbol::from_symbol(s, None).to_minimal_string())
                                .collect::<Vec<_>>()
                                .join(", "),
                        };
                        CallToolResult::success(vec![Content::text(output)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }

            // === 4. search_symbols ===
            "search_symbols" => {
                use crate::index::{CompactFileMeta, CompactSymbol, OutputFormat};

                let params: SearchSymbolsParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let kind_filter = params
                    .kind
                    .and_then(|k| SymbolKind::from_str(&k).map(|kind| vec![kind]));
                let output_format = params
                    .format
                    .as_deref()
                    .and_then(OutputFormat::from_str)
                    .unwrap_or(OutputFormat::Full);
                let fuzzy = params.fuzzy.unwrap_or(false);
                let use_regex = params.regex.unwrap_or(false);
                let file_filter = params.file.clone();
                let include_file_meta = params.include_file_meta.unwrap_or(false);
                let tag_filter = params.tag.clone();

                // If tags are specified, first get files matching those tags
                let tag_filtered_files: Option<std::collections::HashSet<String>> =
                    if let Some(ref tags) = tag_filter {
                        if !tags.is_empty() {
                            match self.index.search_files_by_tags(tags) {
                                Ok(files) => Some(files.into_iter().collect()),
                                Err(_) => None, // Ignore tag filter errors
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                let options = SearchOptions {
                    limit: params.limit.or(Some(20)),
                    kind_filter,
                    language_filter: params.language.map(|l| vec![l]),
                    file_filter: params.file,
                    output_format: Some(output_format),
                    fuzzy: Some(fuzzy),
                    fuzzy_threshold: params.fuzzy_threshold,
                    ..Default::default()
                };

                // Handle regex search
                // Use overlay-first semantics for all searches
                let search_result = if use_regex {
                    self.search_by_pattern_impl(&params.query, file_filter.as_deref(), params.limit)
                        .map(|json| {
                            // Parse back to SearchResult format
                            serde_json::from_str(&json).unwrap_or_default()
                        })
                } else if fuzzy {
                    // Check overlay first, then DB
                    self.overlay
                        .search_with_overlay(&params.query, &self.index, &options)
                        .or_else(|_| self.index.search_fuzzy(&params.query, &options))
                } else {
                    // Overlay-first search: prioritize dirty documents
                    self.overlay
                        .search_with_overlay(&params.query, &self.index, &options)
                };

                match search_result {
                    Ok(mut results) => {
                        // Apply tag filter if specified
                        if let Some(ref allowed_files) = tag_filtered_files {
                            results
                                .retain(|r| allowed_files.contains(&r.symbol.location.file_path));
                        }

                        // Apply max_per_directory diversification if specified
                        if let Some(max_per_dir) = params.max_per_directory {
                            results = diversify_by_directory(results, max_per_dir);
                        }

                        let output = if include_file_meta {
                            // Build results with file metadata
                            let mut items: Vec<serde_json::Value> = Vec::new();
                            let mut file_meta_cache: std::collections::HashMap<
                                String,
                                Option<(crate::index::FileMeta, Vec<crate::index::FileTag>)>,
                            > = std::collections::HashMap::new();

                            for r in &results {
                                let file_path = &r.symbol.location.file_path;

                                // Get cached or fetch file metadata
                                let meta_tags =
                                    file_meta_cache.entry(file_path.clone()).or_insert_with(|| {
                                        self.index.get_file_meta_with_tags(file_path).ok().flatten()
                                    });

                                match output_format {
                                    OutputFormat::Full => {
                                        let mut item = serde_json::to_value(&r).unwrap_or_default();
                                        if let Some((ref meta, ref tags)) = meta_tags {
                                            let fm = CompactFileMeta::from_file_meta(meta, tags);
                                            if let serde_json::Value::Object(ref mut map) = item {
                                                map.insert(
                                                    "fm".to_string(),
                                                    serde_json::to_value(&fm).unwrap_or_default(),
                                                );
                                            }
                                        }
                                        items.push(item);
                                    }
                                    OutputFormat::Compact => {
                                        let mut compact = serde_json::to_value(
                                            CompactSymbol::from_symbol(&r.symbol, Some(r.score)),
                                        )
                                        .unwrap_or_default();
                                        if let Some((ref meta, ref tags)) = meta_tags {
                                            let fm = CompactFileMeta::from_file_meta(meta, tags);
                                            if let serde_json::Value::Object(ref mut map) = compact
                                            {
                                                map.insert(
                                                    "fm".to_string(),
                                                    serde_json::to_value(&fm).unwrap_or_default(),
                                                );
                                            }
                                        }
                                        items.push(compact);
                                    }
                                    OutputFormat::Minimal => {
                                        let minimal =
                                            CompactSymbol::from_symbol(&r.symbol, Some(r.score))
                                                .to_minimal_string();
                                        items.push(serde_json::Value::String(minimal));
                                    }
                                }
                            }

                            serde_json::to_string_pretty(&items).unwrap_or_default()
                        } else {
                            // Original behavior without file metadata
                            match output_format {
                                OutputFormat::Full => {
                                    serde_json::to_string_pretty(&results).unwrap_or_default()
                                }
                                OutputFormat::Compact => {
                                    let compact: Vec<CompactSymbol> = results
                                        .iter()
                                        .map(|r| {
                                            CompactSymbol::from_symbol(&r.symbol, Some(r.score))
                                        })
                                        .collect();
                                    serde_json::to_string(&compact).unwrap_or_default()
                                }
                                OutputFormat::Minimal => results
                                    .iter()
                                    .map(|r| {
                                        CompactSymbol::from_symbol(&r.symbol, Some(r.score))
                                            .to_minimal_string()
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            }
                        };
                        CallToolResult::success(vec![Content::text(output)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }

            // === 5. get_symbol ===
            "get_symbol" => {
                let params: ConsolidatedGetSymbolParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                // Handle batch lookup with overlay-first semantics
                if let Some(ids) = params.ids {
                    let mut symbols = Vec::new();
                    for id in ids {
                        // Try overlay first, then DB
                        if let Ok(Some(symbol)) =
                            self.overlay.get_symbol_with_overlay(&id, &self.index)
                        {
                            symbols.push(symbol);
                        }
                    }
                    let json = serde_json::to_string_pretty(&symbols).unwrap_or_default();
                    return Ok(CallToolResult::success(vec![Content::text(json)]));
                }

                // Handle single ID lookup with overlay-first semantics
                if let Some(id) = params.id {
                    match self.overlay.get_symbol_with_overlay(&id, &self.index) {
                        Ok(Some(symbol)) => {
                            let json = serde_json::to_string_pretty(&symbol).unwrap_or_default();
                            CallToolResult::success(vec![Content::text(json)])
                        }
                        Ok(None) => CallToolResult::error(vec![Content::text(format!(
                            "Symbol not found: {}",
                            id
                        ))]),
                        Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                    }
                }
                // Handle position-based lookup with overlay-first
                else if let (Some(file), Some(line)) = (params.file, params.line) {
                    let column = params.column.unwrap_or(0);

                    // First check overlay for this file/position
                    if let Some(symbol) = self.overlay.get_symbol_at_position(&file, line, column) {
                        let json = serde_json::to_string_pretty(&vec![symbol]).unwrap_or_default();
                        return Ok(CallToolResult::success(vec![Content::text(json)]));
                    }

                    // Fall back to DB
                    match self.index.get_file_symbols(&file) {
                        Ok(symbols) => {
                            let found: Vec<_> = symbols
                                .into_iter()
                                .filter(|s| {
                                    s.location.start_line <= line
                                        && s.location.end_line >= line
                                        && (column == 0
                                            || (s.location.start_column <= column
                                                && s.location.end_column >= column))
                                })
                                .collect();
                            let json = serde_json::to_string_pretty(&found).unwrap_or_default();
                            CallToolResult::success(vec![Content::text(json)])
                        }
                        Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                    }
                } else {
                    CallToolResult::error(vec![Content::text(
                        "Must provide 'id', 'ids', or 'file' + 'line'",
                    )])
                }
            }

            // === 6. find_definitions ===
            "find_definitions" => {
                let params: FindDefinitionsParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let include_deps = params.include_deps.unwrap_or(false);

                let result = if include_deps {
                    self.index
                        .find_definition_in_dependencies(&params.name, params.dependency.as_deref())
                } else {
                    self.index.find_definition(&params.name)
                };

                match result {
                    Ok(symbols) => {
                        let json = serde_json::to_string_pretty(&symbols).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }

            // === 7. find_references ===
            "find_references" => {
                let params: ConsolidatedFindReferencesParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let include_callers = params.include_callers.unwrap_or(false);
                let include_importers = params.include_importers.unwrap_or(false);

                let options = SearchOptions {
                    limit: params.limit,
                    file_filter: params.file,
                    ..Default::default()
                };

                let mut output = serde_json::Map::new();

                // Basic references
                if let Ok(refs) = self.index.find_references(&params.name, &options) {
                    output.insert(
                        "references".to_string(),
                        serde_json::to_value(&refs).unwrap_or_default(),
                    );
                }

                // Include callers if requested
                if include_callers {
                    let depth = params.depth;
                    if let Ok(callers) = self.index.find_callers(&params.name, depth) {
                        output.insert(
                            "callers".to_string(),
                            serde_json::to_value(&callers).unwrap_or_default(),
                        );
                    }
                }

                // Include importers if requested
                if include_importers {
                    // Try to find files that import this symbol
                    if let Ok(definitions) = self.index.find_definition(&params.name) {
                        for def in definitions {
                            if let Ok(importers) =
                                self.index.get_file_importers(&def.location.file_path)
                            {
                                output.insert(
                                    "importers".to_string(),
                                    serde_json::to_value(&importers).unwrap_or_default(),
                                );
                                break;
                            }
                        }
                    }
                }

                let json = serde_json::to_string_pretty(&serde_json::Value::Object(output.clone()))
                    .unwrap_or_default();
                CallToolResult::success(vec![Content::text(json)])
            }

            // === 8. analyze_call_graph ===
            "analyze_call_graph" => {
                let params: AnalyzeCallGraphParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let depth = params.depth.unwrap_or(3);
                let direction = params.direction.as_deref().unwrap_or("out");

                let mut output = serde_json::Map::new();

                // Outgoing calls (callees)
                if direction == "out" || direction == "both" {
                    match self.index.get_call_graph(&params.function, depth) {
                        Ok(graph) => {
                            output.insert(
                                "call_graph".to_string(),
                                serde_json::to_value(&graph).unwrap_or_default(),
                            );
                        }
                        Err(e) => {
                            output.insert(
                                "call_graph_error".to_string(),
                                serde_json::Value::String(e.to_string()),
                            );
                        }
                    }
                }

                // Incoming calls (callers)
                if direction == "in" || direction == "both" {
                    match self.index.find_callers(&params.function, Some(depth)) {
                        Ok(callers) => {
                            output.insert(
                                "callers".to_string(),
                                serde_json::to_value(&callers).unwrap_or_default(),
                            );
                        }
                        Err(e) => {
                            output.insert(
                                "callers_error".to_string(),
                                serde_json::Value::String(e.to_string()),
                            );
                        }
                    }
                }

                let json = serde_json::to_string_pretty(&serde_json::Value::Object(output))
                    .unwrap_or_default();
                CallToolResult::success(vec![Content::text(json)])
            }

            // === 9. get_file_outline ===
            "get_file_outline" => {
                let params: GetFileOutlineParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.get_file_symbols(&params.file) {
                    Ok(symbols) => {
                        // Filter by line range if specified
                        let filtered: Vec<_> =
                            if params.start_line.is_some() || params.end_line.is_some() {
                                let start = params.start_line.unwrap_or(0);
                                let end = params.end_line.unwrap_or(u32::MAX);
                                symbols
                                    .into_iter()
                                    .filter(|s| {
                                        s.location.start_line >= start && s.location.end_line <= end
                                    })
                                    .collect()
                            } else {
                                symbols
                            };

                        // Include scopes if requested
                        let mut output = serde_json::Map::new();
                        output.insert(
                            "symbols".to_string(),
                            serde_json::to_value(&filtered).unwrap_or_default(),
                        );

                        if params.include_scopes.unwrap_or(false) {
                            if let Ok(scopes) = self.index.get_file_scopes(&params.file) {
                                output.insert(
                                    "scopes".to_string(),
                                    serde_json::to_value(&scopes).unwrap_or_default(),
                                );
                            }
                        }

                        // Include file metadata (Intent Layer) if requested
                        if params.include_file_meta.unwrap_or(false) {
                            if let Ok(Some((meta, tags))) =
                                self.index.get_file_meta_with_tags(&params.file)
                            {
                                // Build compact file_meta object
                                let mut file_meta = serde_json::Map::new();
                                if let Some(ref d1) = meta.doc1 {
                                    file_meta.insert(
                                        "doc1".to_string(),
                                        serde_json::Value::String(d1.clone()),
                                    );
                                }
                                if let Some(ref purpose) = meta.purpose {
                                    file_meta.insert(
                                        "purpose".to_string(),
                                        serde_json::Value::String(purpose.clone()),
                                    );
                                }
                                if !meta.capabilities.is_empty() {
                                    file_meta.insert(
                                        "capabilities".to_string(),
                                        serde_json::to_value(&meta.capabilities)
                                            .unwrap_or_default(),
                                    );
                                }
                                if !meta.invariants.is_empty() {
                                    file_meta.insert(
                                        "invariants".to_string(),
                                        serde_json::to_value(&meta.invariants).unwrap_or_default(),
                                    );
                                }
                                if let Some(stability) = meta.stability {
                                    file_meta.insert(
                                        "stability".to_string(),
                                        serde_json::Value::String(stability.as_str().to_string()),
                                    );
                                }
                                if let Some(ref owner) = meta.owner {
                                    file_meta.insert(
                                        "owner".to_string(),
                                        serde_json::Value::String(owner.clone()),
                                    );
                                }

                                // Add tags as category:name format
                                let tag_strs: Vec<String> = tags
                                    .iter()
                                    .filter_map(|t| {
                                        t.tag_category.as_ref().and_then(|cat| {
                                            t.tag_name
                                                .as_ref()
                                                .map(|name| format!("{}:{}", cat, name))
                                        })
                                    })
                                    .collect();
                                if !tag_strs.is_empty() {
                                    file_meta.insert(
                                        "tags".to_string(),
                                        serde_json::to_value(&tag_strs).unwrap_or_default(),
                                    );
                                }

                                // Add staleness info
                                if meta.is_stale {
                                    file_meta.insert(
                                        "is_stale".to_string(),
                                        serde_json::Value::Bool(true),
                                    );
                                }
                                if let Some(ref hash) = meta.exported_hash {
                                    file_meta.insert(
                                        "exported_hash".to_string(),
                                        serde_json::Value::String(hash.clone()),
                                    );
                                }

                                output.insert(
                                    "file_meta".to_string(),
                                    serde_json::Value::Object(file_meta),
                                );
                            }
                        }

                        let json = serde_json::to_string_pretty(&serde_json::Value::Object(output))
                            .unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }

            // === 10. get_imports ===
            "get_imports" => {
                let params: GetImportsParams = serde_json::from_value(serde_json::Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.get_file_imports(&params.file) {
                    Ok(imports) => {
                        let mut output = serde_json::Map::new();
                        output.insert(
                            "imports".to_string(),
                            serde_json::to_value(&imports).unwrap_or_default(),
                        );

                        // Resolve imports if requested
                        if params.resolve.unwrap_or(false) {
                            let mut resolved = Vec::new();
                            for import in &imports {
                                if let Some(ref symbol_name) = import.imported_symbol {
                                    if let Ok(definitions) = self.index.find_definition(symbol_name)
                                    {
                                        resolved.push(serde_json::json!({
                                            "import": import,
                                            "definitions": definitions,
                                        }));
                                    }
                                }
                            }
                            output.insert(
                                "resolved".to_string(),
                                serde_json::to_value(&resolved).unwrap_or_default(),
                            );
                        }

                        let json = serde_json::to_string_pretty(&serde_json::Value::Object(output))
                            .unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }

            // === 11. get_diagnostics ===
            "get_diagnostics" => {
                let params: GetDiagnosticsParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let kind = params.kind.as_deref().unwrap_or("all");
                let mut output = serde_json::Map::new();

                // Dead code detection
                if kind == "dead_code" || kind == "all" {
                    match self.index.find_dead_code() {
                        Ok(report) => {
                            output.insert(
                                "dead_code".to_string(),
                                serde_json::to_value(&report).unwrap_or_default(),
                            );
                        }
                        Err(e) => {
                            output.insert(
                                "dead_code_error".to_string(),
                                serde_json::Value::String(e.to_string()),
                            );
                        }
                    }
                }

                // Include metrics if requested
                if params.include_metrics.unwrap_or(false) {
                    if let Some(ref target) = params.target {
                        // Try as function first, then as file
                        match self.index.get_function_metrics(target) {
                            Ok(metrics) => {
                                output.insert(
                                    "metrics".to_string(),
                                    serde_json::to_value(&metrics).unwrap_or_default(),
                                );
                            }
                            Err(_) => {
                                if let Ok(metrics) = self.index.get_file_metrics(target) {
                                    output.insert(
                                        "metrics".to_string(),
                                        serde_json::to_value(&metrics).unwrap_or_default(),
                                    );
                                }
                            }
                        }
                    }
                }

                let json = serde_json::to_string_pretty(&serde_json::Value::Object(output))
                    .unwrap_or_default();
                CallToolResult::success(vec![Content::text(json)])
            }

            // === 12. get_stats ===
            "get_stats" => {
                let params: GetStatsParams = serde_json::from_value(serde_json::Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.get_stats() {
                    Ok(stats) => {
                        let mut output = serde_json::to_value(&stats).unwrap_or_default();

                        // Add workspace info if requested
                        if params.include_workspace.unwrap_or(false) {
                            if let Ok(modules) = self.list_modules_impl(".") {
                                if let Ok(modules_json) =
                                    serde_json::from_str::<serde_json::Value>(&modules)
                                {
                                    if let serde_json::Value::Object(ref mut map) = output {
                                        map.insert("workspace".to_string(), modules_json);
                                    }
                                }
                            }
                        }

                        // Add architecture summary if requested
                        if params.include_architecture.unwrap_or(false) {
                            if let Ok(arch) = self.get_architecture_summary_impl(".") {
                                if let Ok(arch_json) =
                                    serde_json::from_str::<serde_json::Value>(&arch)
                                {
                                    if let serde_json::Value::Object(ref mut map) = output {
                                        map.insert("architecture".to_string(), arch_json);
                                    }
                                }
                            }
                        }

                        let json = serde_json::to_string_pretty(&output).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }

            // === 13. get_context_bundle (Summary-First Contract) ===
            "get_context_bundle" => {
                let params: GetContextBundleParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.get_context_bundle_impl(params) {
                    Ok(result) => {
                        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
                        CallToolResult::success(vec![Content::text(json)])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }

            // === Legacy tools kept for backward compatibility ===
            // These are handled by the alias mapping above, but we keep explicit handlers
            // for tools that have slightly different parameter structures
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
            // === Additional legacy tools that need explicit handling ===
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

            "list_modules" => {
                let params: ListModulesParams = serde_json::from_value(serde_json::Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
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

            "get_changed_symbols" => {
                use crate::index::OutputFormat;
                use code_indexer::git::GitAnalyzer;

                let params: GetChangedSymbolsParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let base = params.base.as_deref().unwrap_or("HEAD");
                let include_staged = params.include_staged.unwrap_or(true);
                let include_unstaged = params.include_unstaged.unwrap_or(true);
                let output_format = params
                    .format
                    .and_then(|f| OutputFormat::from_str(&f))
                    .unwrap_or(OutputFormat::Full);

                match GitAnalyzer::new(".") {
                    Ok(git) => {
                        match git.find_changed_symbols(
                            self.index.as_ref(),
                            base,
                            include_staged,
                            include_unstaged,
                        ) {
                            Ok(changed) => {
                                let output = match output_format {
                                    OutputFormat::Full => {
                                        serde_json::to_string_pretty(&changed).unwrap_or_default()
                                    }
                                    OutputFormat::Compact => {
                                        let compact: Vec<serde_json::Value> = changed
                                            .iter()
                                            .map(|cs| {
                                                serde_json::json!({
                                                    "n": cs.symbol.name,
                                                    "k": cs.symbol.kind.short_str(),
                                                    "f": cs.symbol.location.file_path,
                                                    "l": cs.symbol.location.start_line,
                                                    "st": cs.file_status
                                                })
                                            })
                                            .collect();
                                        serde_json::to_string(&compact).unwrap_or_default()
                                    }
                                    OutputFormat::Minimal => changed
                                        .iter()
                                        .map(|cs| {
                                            format!(
                                                "{}:{}@{}:{} [{}]",
                                                cs.symbol.name,
                                                cs.symbol.kind.short_str(),
                                                cs.symbol.location.file_path,
                                                cs.symbol.location.start_line,
                                                cs.file_status
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                };
                                CallToolResult::success(vec![Content::text(output)])
                            }
                            Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                        }
                    }
                    Err(e) => {
                        CallToolResult::error(vec![Content::text(format!("Git error: {}", e))])
                    }
                }
            }

            // === 14. get_snippet ===
            "get_snippet" => {
                let params: GetSnippetParams = serde_json::from_value(serde_json::Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let context_lines = params.context_lines.unwrap_or(3);
                let max_lines = params.max_lines.unwrap_or(50);
                let expand_to_scope = params.expand_to_scope.unwrap_or(false);
                let redact = params.redact.unwrap_or(true); // Default to redact for safety

                // Parse target: either stable_id or file:line
                let (file_path, start_line, end_line) = if params.target.starts_with("sid:") {
                    // Lookup by stable_id
                    match self.index.get_symbol_by_stable_id(&params.target) {
                        Ok(Some(symbol)) => (
                            symbol.location.file_path.clone(),
                            symbol.location.start_line,
                            if expand_to_scope {
                                Some(symbol.location.end_line)
                            } else {
                                None
                            },
                        ),
                        Ok(None) => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Symbol not found: {}",
                                params.target
                            ))]));
                        }
                        Err(e) => {
                            return Ok(CallToolResult::error(vec![Content::text(e.to_string())]));
                        }
                    }
                } else if let Some((file, line_str)) = params.target.rsplit_once(':') {
                    // Parse file:line format
                    match line_str.parse::<u32>() {
                        Ok(line) => (file.to_string(), line, None),
                        Err(_) => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Invalid target format: {}. Expected 'sid:...' or 'file:line'",
                                params.target
                            ))]));
                        }
                    }
                } else {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Invalid target format: {}. Expected 'sid:...' or 'file:line'",
                        params.target
                    ))]));
                };

                // Read the file and extract snippet
                match std::fs::read_to_string(&file_path) {
                    Ok(content) => {
                        let lines: Vec<&str> = content.lines().collect();
                        let total_lines = lines.len();

                        // Calculate line range
                        let start_with_context =
                            (start_line.saturating_sub(1) as usize).saturating_sub(context_lines);
                        let base_end = if let Some(end) = end_line {
                            end as usize
                        } else {
                            start_line as usize
                        };
                        let end_with_context = (base_end + context_lines).min(total_lines);

                        // Apply max_lines limit
                        let actual_end = (start_with_context + max_lines)
                            .min(end_with_context)
                            .min(total_lines);

                        let snippet_lines: Vec<String> = lines[start_with_context..actual_end]
                            .iter()
                            .enumerate()
                            .map(|(i, line)| {
                                let line_content = if redact {
                                    redact_secrets(line)
                                } else {
                                    line.to_string()
                                };
                                format!("{:4} | {}", start_with_context + i + 1, line_content)
                            })
                            .collect();

                        let output = serde_json::json!({
                            "file": file_path,
                            "start_line": start_with_context + 1,
                            "end_line": actual_end,
                            "total_lines": actual_end - start_with_context,
                            "truncated": actual_end < end_with_context,
                            "redacted": redact,
                            "snippet": snippet_lines.join("\n"),
                        });

                        CallToolResult::success(vec![Content::text(
                            serde_json::to_string_pretty(&output).unwrap_or_default(),
                        )])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(format!(
                        "Failed to read file '{}': {}",
                        file_path, e
                    ))]),
                }
            }

            "get_doc_section" => {
                let params: GetDocSectionParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let include_code = params.include_code.unwrap_or(false);

                // Try to find the document by path or type
                let digest = if params.target.contains('/') || params.target.contains('.') {
                    // Target is a file path
                    self.index
                        .get_doc_digest(&params.target)
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?
                } else {
                    // Target is a doc type (readme, contributing, etc.)
                    let all_docs = self
                        .index
                        .get_all_doc_digests()
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    let target_lower = params.target.to_lowercase();
                    all_docs
                        .into_iter()
                        .find(|d| d.doc_type.as_str() == target_lower)
                };

                match digest {
                    Some(doc) => {
                        let available_sections: Vec<String> =
                            doc.headings.iter().map(|h| h.text.clone()).collect();

                        let (section_content, code_blocks) =
                            if let Some(ref section_name) = params.section {
                                // Find and extract the specific section
                                let file_content = std::fs::read_to_string(&doc.file_path).ok();
                                let content = file_content.as_ref().and_then(|c| {
                                    code_indexer::docs::DocParser::extract_section(c, section_name)
                                });

                                let blocks: Vec<DocCodeBlock> = if include_code {
                                    // Find code blocks within the section
                                    doc.command_blocks
                                        .iter()
                                        .filter(|b| {
                                            // Check if block is within the section
                                            let section = doc.key_sections.iter().find(|s| {
                                                s.heading
                                                    .to_lowercase()
                                                    .contains(&section_name.to_lowercase())
                                            });
                                            if let Some(s) = section {
                                                b.line >= s.start_line && b.line < s.end_line
                                            } else {
                                                false
                                            }
                                        })
                                        .map(|b| DocCodeBlock {
                                            language: b.language.clone(),
                                            content: b.content.clone(),
                                            line: b.line,
                                        })
                                        .collect()
                                } else {
                                    Vec::new()
                                };

                                (content, blocks)
                            } else {
                                // Return all command blocks if no section specified
                                let blocks: Vec<DocCodeBlock> = if include_code {
                                    doc.command_blocks
                                        .iter()
                                        .map(|b| DocCodeBlock {
                                            language: b.language.clone(),
                                            content: b.content.clone(),
                                            line: b.line,
                                        })
                                        .collect()
                                } else {
                                    Vec::new()
                                };
                                (None, blocks)
                            };

                        let response = DocSectionResponse {
                            file_path: doc.file_path,
                            doc_type: doc.doc_type.as_str().to_string(),
                            title: doc.title,
                            available_sections,
                            section_content,
                            code_blocks,
                        };

                        CallToolResult::success(vec![Content::text(
                            serde_json::to_string_pretty(&response).unwrap_or_default(),
                        )])
                    }
                    None => {
                        // List available documentation files
                        let all_docs = self
                            .index
                            .get_all_doc_digests()
                            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                        let available: Vec<String> = all_docs
                            .iter()
                            .map(|d| format!("{} ({})", d.file_path, d.doc_type.as_str()))
                            .collect();

                        CallToolResult::error(vec![Content::text(format!(
                            "Document '{}' not found. Available documents: {:?}",
                            params.target, available
                        ))])
                    }
                }
            }

            "get_project_commands" => {
                let params: GetProjectCommandsParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                match self.index.get_project_commands() {
                    Ok(commands) => {
                        let response = match params.kind.as_deref() {
                            Some("run") => ProjectCommandsResponse {
                                run: commands.run,
                                build: Vec::new(),
                                test: Vec::new(),
                            },
                            Some("build") => ProjectCommandsResponse {
                                run: Vec::new(),
                                build: commands.build,
                                test: Vec::new(),
                            },
                            Some("test") => ProjectCommandsResponse {
                                run: Vec::new(),
                                build: Vec::new(),
                                test: commands.test,
                            },
                            _ => ProjectCommandsResponse {
                                run: commands.run,
                                build: commands.build,
                                test: commands.test,
                            },
                        };

                        CallToolResult::success(vec![Content::text(
                            serde_json::to_string_pretty(&response).unwrap_or_default(),
                        )])
                    }
                    Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
                }
            }

            "get_project_compass" => {
                let params: GetProjectCompassParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let max_bytes = params.max_bytes.unwrap_or(16 * 1024);
                let include_entry_points = params.include_entry_points.unwrap_or(true);
                let include_modules = params.include_modules.unwrap_or(true);
                let include_docs = params.include_docs.unwrap_or(true);

                // Get database revision
                let db_rev = self.index.get_db_revision().unwrap_or(0);

                // Build or get project profile
                let (profile, profile_rev) =
                    match code_indexer::compass::ProfileBuilder::build(self.index.as_ref()) {
                        Ok(p) => {
                            // Save to database for caching
                            let _ = self.index.save_project_profile(".", &p);
                            (p, db_rev)
                        }
                        Err(_) => {
                            // Try to get cached profile
                            match self.index.get_project_profile(".") {
                                Ok(Some((p, rev))) => (p, rev),
                                _ => {
                                    return Ok(CallToolResult::error(vec![Content::text(
                                        "Failed to build project profile".to_string(),
                                    )]));
                                }
                            }
                        }
                    };

                // Convert profile to compass format
                let compass_profile = CompassProfile {
                    languages: profile
                        .languages
                        .iter()
                        .map(|l| CompassLanguage {
                            name: l.name.clone(),
                            files: l.file_count,
                            symbols: l.symbol_count,
                            pct: l.percentage,
                        })
                        .collect(),
                    frameworks: profile.frameworks.iter().map(|f| f.name.clone()).collect(),
                    build_tools: profile.build_tools.clone(),
                    workspace_type: profile.workspace_type.clone(),
                };

                // Get commands
                let commands = self
                    .index
                    .get_project_commands()
                    .ok()
                    .map(|c| CompassCommands {
                        run: c.run,
                        build: c.build,
                        test: c.test,
                    });

                // Get entry points
                let entry_points = if include_entry_points {
                    code_indexer::compass::EntryDetector::detect(self.index.as_ref())
                        .unwrap_or_default()
                        .into_iter()
                        .take(10) // Limit for budget
                        .map(|e| CompassEntryPoint {
                            name: e.name,
                            entry_type: e.entry_type.as_str().to_string(),
                            file: e.file_path,
                            line: e.line,
                            evidence: Some(e.evidence),
                        })
                        .collect()
                } else {
                    Vec::new()
                };

                // Get module hierarchy
                let modules_top = if include_modules {
                    let nodes = code_indexer::compass::NodeBuilder::build(self.index.as_ref(), ".")
                        .unwrap_or_default();
                    let _ = self.index.save_project_nodes(&nodes);

                    code_indexer::compass::NodeBuilder::get_top_level(&nodes)
                        .into_iter()
                        .take(10) // Limit for budget
                        .map(|n| CompassModuleNode {
                            id: n.id.clone(),
                            node_type: n.node_type.as_str().to_string(),
                            name: n.name.clone(),
                            path: n.path.clone(),
                            symbol_count: n.symbol_count,
                            file_count: n.file_count,
                        })
                        .collect()
                } else {
                    Vec::new()
                };

                // Get documentation info
                let docs = if include_docs {
                    let all_docs = self.index.get_all_doc_digests().unwrap_or_default();

                    let readme_headings = all_docs
                        .iter()
                        .find(|d| d.doc_type == code_indexer::docs::DocType::Readme)
                        .map(|d| {
                            d.headings
                                .iter()
                                .filter(|h| h.level <= 2)
                                .map(|h| h.text.clone())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();

                    let has_contributing = all_docs
                        .iter()
                        .any(|d| d.doc_type == code_indexer::docs::DocType::Contributing);
                    let has_changelog = all_docs
                        .iter()
                        .any(|d| d.doc_type == code_indexer::docs::DocType::Changelog);

                    Some(CompassDocs {
                        readme_headings,
                        has_contributing,
                        has_changelog,
                    })
                } else {
                    None
                };

                // Build next actions
                let mut next = Vec::new();
                if !modules_top.is_empty() {
                    next.push(CompassNextAction {
                        tool: "expand_project_node".to_string(),
                        args: serde_json::json!({"node_id": modules_top[0].id}),
                        description: Some(format!("Explore {} module", modules_top[0].name)),
                    });
                }
                if !entry_points.is_empty() {
                    next.push(CompassNextAction {
                        tool: "get_snippet".to_string(),
                        args: serde_json::json!({"target": format!("{}:{}", entry_points[0].file, entry_points[0].line)}),
                        description: Some("View main entry point".to_string()),
                    });
                }

                // Build response
                let response = ProjectCompassResponse {
                    meta: CompassMeta {
                        db_rev,
                        profile_rev,
                        budget: CompassBudget {
                            actual_bytes: 0, // Will be updated
                            max_bytes: Some(max_bytes),
                        },
                    },
                    profile: compass_profile,
                    commands,
                    entry_points,
                    modules_top,
                    docs,
                    next,
                };

                // Serialize and check budget
                let json_output = serde_json::to_string_pretty(&response).unwrap_or_default();
                let actual_bytes = json_output.len();

                // Update actual_bytes in response
                let mut final_response = response;
                final_response.meta.budget.actual_bytes = actual_bytes;

                let final_output =
                    serde_json::to_string_pretty(&final_response).unwrap_or_default();

                CallToolResult::success(vec![Content::text(final_output)])
            }

            "expand_project_node" => {
                let params: ExpandProjectNodeParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let limit = params.limit.unwrap_or(20);
                let include_symbols = params.include_symbols.unwrap_or(false);

                // Get the node
                let node = match self.index.get_project_node(&params.node_id) {
                    Ok(Some(n)) => n,
                    Ok(None) => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Node not found: {}",
                            params.node_id
                        ))]));
                    }
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(e.to_string())]));
                    }
                };

                // Get children
                let children: Vec<CompassModuleNode> = self
                    .index
                    .get_node_children(&params.node_id)
                    .unwrap_or_default()
                    .into_iter()
                    .take(limit)
                    .map(|n| CompassModuleNode {
                        id: n.id,
                        node_type: n.node_type.as_str().to_string(),
                        name: n.name,
                        path: n.path,
                        symbol_count: n.symbol_count,
                        file_count: n.file_count,
                    })
                    .collect();

                // Get files in this node's path
                let top_files: Vec<NodeFileInfo> = if let Ok(stats) = self.index.get_stats() {
                    // Create file info from languages found in this node
                    stats
                        .files_by_language
                        .iter()
                        .filter(|(_, count)| *count > 0)
                        .take(limit)
                        .map(|(lang, _)| {
                            let symbol_count = stats
                                .symbols_by_language
                                .iter()
                                .find(|(l, _)| l == lang)
                                .map(|(_, c)| *c)
                                .unwrap_or(0);
                            NodeFileInfo {
                                path: format!("{}/*.{}", node.path, lang),
                                language: lang.clone(),
                                symbol_count,
                            }
                        })
                        .collect()
                } else {
                    Vec::new()
                };

                // Get top symbols if requested
                let top_symbols: Vec<SymbolCard> = if include_symbols {
                    let sym_options = crate::index::SearchOptions {
                        file_filter: Some(node.path.clone()),
                        limit: Some(limit),
                        ..Default::default()
                    };

                    let functions = self.index.list_functions(&sym_options).unwrap_or_default();
                    let types = self.index.list_types(&sym_options).unwrap_or_default();

                    functions
                        .into_iter()
                        .chain(types.into_iter())
                        .take(limit)
                        .enumerate()
                        .map(|(i, s)| SymbolCard {
                            id: s.id.clone(),
                            fqdn: s.fqdn.clone(),
                            kind: s.kind.as_str().to_string(),
                            sig: s.signature.clone(),
                            loc: format!("{}:{}", s.location.file_path, s.location.start_line),
                            rank: (i + 1) as u32,
                            snippet: None,
                        })
                        .collect()
                } else {
                    Vec::new()
                };

                let response = ExpandedNodeResponse {
                    node: CompassModuleNode {
                        id: node.id,
                        node_type: node.node_type.as_str().to_string(),
                        name: node.name,
                        path: node.path,
                        symbol_count: node.symbol_count,
                        file_count: node.file_count,
                    },
                    children,
                    top_files,
                    top_symbols,
                    next_cursor: None, // TODO: implement pagination
                };

                CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response).unwrap_or_default(),
                )])
            }

            "get_compass" => {
                let params: GetCompassQueryParams = serde_json::from_value(
                    serde_json::Value::Object(request.arguments.unwrap_or_default()),
                )
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let limit = params.limit.unwrap_or(10);
                let query = &params.query;
                let query_lower = query.to_lowercase();

                let mut results: Vec<CompassResult> = Vec::new();
                let mut total_matches = 0usize;

                // 1. Search symbols (max 4)
                let sym_options = crate::index::SearchOptions {
                    limit: Some(20),
                    current_file: params.current_file.clone(),
                    ..Default::default()
                };
                if let Ok(search_results) = self.index.search(query, &sym_options) {
                    total_matches += search_results.len();
                    for result in search_results.into_iter().take(4) {
                        let symbol = result.symbol;
                        results.push(CompassResult {
                            result_type: "symbol".to_string(),
                            name: symbol.name.clone(),
                            path: symbol.location.file_path.clone(),
                            why: format!("symbol name matches '{}'", query),
                            score: result.score as f32,
                            symbol_id: Some(symbol.id.clone()),
                            line: Some(symbol.location.start_line),
                        });
                    }
                }

                // 2. Search modules (max 2)
                let nodes = self.index.get_project_nodes().unwrap_or_default();
                for node in nodes.iter().take(50) {
                    if node.name.to_lowercase().contains(&query_lower)
                        || node.path.to_lowercase().contains(&query_lower)
                    {
                        total_matches += 1;
                        if results.iter().filter(|r| r.result_type == "module").count() < 2 {
                            results.push(CompassResult {
                                result_type: "module".to_string(),
                                name: node.name.clone(),
                                path: node.path.clone(),
                                why: format!("module path contains '{}'", query),
                                score: 0.8,
                                symbol_id: None,
                                line: None,
                            });
                        }
                    }
                }

                // 3. Search docs (max 1)
                if let Ok(docs) = self.index.get_all_doc_digests() {
                    for doc in docs {
                        // Search in headings
                        for heading in &doc.headings {
                            if heading.text.to_lowercase().contains(&query_lower) {
                                total_matches += 1;
                                if results.iter().filter(|r| r.result_type == "doc").count() < 1 {
                                    results.push(CompassResult {
                                        result_type: "doc".to_string(),
                                        name: heading.text.clone(),
                                        path: doc.file_path.clone(),
                                        why: format!("doc heading contains '{}'", query),
                                        score: 0.7,
                                        symbol_id: None,
                                        line: Some(heading.line),
                                    });
                                }
                                break;
                            }
                        }
                    }
                }

                // 4. Search commands (max 1)
                if let Ok(commands) = self.index.get_project_commands() {
                    for cmd in commands
                        .run
                        .iter()
                        .chain(commands.build.iter())
                        .chain(commands.test.iter())
                    {
                        if cmd.to_lowercase().contains(&query_lower) {
                            total_matches += 1;
                            if results
                                .iter()
                                .filter(|r| r.result_type == "command")
                                .count()
                                < 1
                            {
                                results.push(CompassResult {
                                    result_type: "command".to_string(),
                                    name: cmd.clone(),
                                    path: "project commands".to_string(),
                                    why: format!("command contains '{}'", query),
                                    score: 0.6,
                                    symbol_id: None,
                                    line: None,
                                });
                                break;
                            }
                        }
                    }
                }

                // Sort by score and limit
                results.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                results.truncate(limit);

                let response = CompassQueryResponse {
                    query: query.clone(),
                    results,
                    total_matches,
                };

                CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response).unwrap_or_default(),
                )])
            }

            "open_session" => {
                let params: OpenSessionParams = serde_json::from_value(serde_json::Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let session = self
                    .session_manager
                    .open_session(params.restore_session.as_deref());
                let restored = params.restore_session.is_some();

                let delta = session.get_dict();

                let response = SessionResponse {
                    session_id: session.id,
                    dict: SessionDict {
                        files: delta.files,
                        kinds: delta.kinds,
                        modules: delta.modules,
                    },
                    restored,
                };

                CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response).unwrap_or_default(),
                )])
            }

            "close_session" => {
                let params: CloseSessionParams = serde_json::from_value(serde_json::Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let closed = self.session_manager.close_session(&params.session_id);

                let response = CloseSessionResponse { closed };

                CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response).unwrap_or_default(),
                )])
            }

            "manage_tags" => {
                let params: ManageTagsParams = serde_json::from_value(serde_json::Value::Object(
                    request.arguments.unwrap_or_default(),
                ))
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

                let response = self.handle_manage_tags(params)?;

                CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response).unwrap_or_default(),
                )])
            }

            "get_indexing_status" => {
                let snap = self.indexing_progress.snapshot();
                let output = serde_json::json!({
                    "is_active": snap.is_active,
                    "files_total": snap.files_total,
                    "files_processed": snap.files_processed,
                    "symbols_extracted": snap.symbols_extracted,
                    "errors": snap.errors,
                    "progress_pct": (snap.progress_pct * 10.0).round() / 10.0,
                    "elapsed_ms": snap.elapsed_ms,
                    "eta_ms": snap.eta_ms,
                });
                CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&output).unwrap_or_default(),
                )])
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

#[cfg(test)]
mod tests {
    use super::*;

    // === enforce_max_bytes tests ===

    #[test]
    fn test_enforce_max_bytes_under_limit() {
        let output = "Hello, World!";
        let (result, truncated) = enforce_max_bytes(output, Some(100));
        assert_eq!(result, output);
        assert!(!truncated);
    }

    #[test]
    fn test_enforce_max_bytes_at_limit() {
        let output = "Hello";
        let (result, truncated) = enforce_max_bytes(output, Some(5));
        assert_eq!(result, output);
        assert!(!truncated);
    }

    #[test]
    fn test_enforce_max_bytes_over_limit() {
        let output = "Hello, World!";
        let (result, truncated) = enforce_max_bytes(output, Some(8));
        assert!(result.ends_with("..."));
        assert!(result.len() <= 8);
        assert!(truncated);
    }

    #[test]
    fn test_enforce_max_bytes_none_limit() {
        let output = "Hello, World! This is a very long string that should not be truncated.";
        let (result, truncated) = enforce_max_bytes(output, None);
        assert_eq!(result, output);
        assert!(!truncated);
    }

    #[test]
    fn test_enforce_max_bytes_utf8_boundary() {
        let output = "Привет мир"; // Cyrillic characters (2 bytes each)
        let (result, truncated) = enforce_max_bytes(output, Some(10));
        // Should truncate at safe boundary and add "..."
        assert!(truncated);
        assert!(result.ends_with("..."));
        // Verify the result is valid UTF-8
        assert!(result.is_ascii() || result.chars().all(|c| c.len_utf8() >= 1));
    }

    #[test]
    fn test_enforce_max_bytes_very_small_limit() {
        let output = "Hello, World!";
        let (result, truncated) = enforce_max_bytes(output, Some(2));
        assert_eq!(result, "...");
        assert!(truncated);
    }

    // === enforce_budget_on_items tests ===

    #[test]
    fn test_enforce_budget_under_limit() {
        let items = vec!["a", "b", "c"];
        let (result, total, truncated) = enforce_budget_on_items(items.clone(), Some(10), None);
        assert_eq!(result, items);
        assert_eq!(total, 3);
        assert!(!truncated);
    }

    #[test]
    fn test_enforce_budget_truncated_by_items() {
        let items = vec!["a", "b", "c", "d", "e"];
        let (result, total, truncated) = enforce_budget_on_items(items, Some(3), None);
        assert_eq!(result.len(), 3);
        assert_eq!(total, 5);
        assert!(truncated);
    }

    #[test]
    fn test_enforce_budget_truncated_by_bytes() {
        let items = vec!["long_item_1", "long_item_2", "long_item_3"];
        let (result, total, truncated) = enforce_budget_on_items(items, None, Some(30));
        // Serialized JSON will be truncated by bytes
        assert!(total == 3);
        // Result should be truncated to fit within ~30 bytes
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(serialized.len() <= 30 || result.is_empty());
        assert!(truncated || result.len() == 3);
    }

    #[test]
    fn test_enforce_budget_both_limits() {
        let items = vec!["item1", "item2", "item3", "item4", "item5"];
        let (result, total, truncated) = enforce_budget_on_items(items, Some(4), Some(20));
        assert!(result.len() <= 4);
        assert_eq!(total, 5);
        assert!(truncated);
    }

    #[test]
    fn test_enforce_budget_empty_input() {
        let items: Vec<&str> = vec![];
        let (result, total, truncated) = enforce_budget_on_items(items, Some(10), Some(100));
        assert!(result.is_empty());
        assert_eq!(total, 0);
        assert!(!truncated);
    }

    // === redact_secrets tests ===

    #[test]
    fn test_redact_secrets_api_key() {
        // Build the test key dynamically to avoid triggering GitHub secret scanning
        let test_key = format!("{}_{}_abcdefghijklmnopqrstuvwxyz1234567890", "sk", "live");
        let content = format!(r#"let api_key = "{}";"#, test_key);
        let result = redact_secrets(&content);
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("sk_live_"));
    }

    #[test]
    fn test_redact_secrets_bearer_token() {
        let content = r#"Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ"#;
        let result = redact_secrets(content);
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_redact_secrets_password_env() {
        let content = r#"password = "super_secret_password123""#;
        let result = redact_secrets(content);
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("super_secret"));
    }

    #[test]
    fn test_redact_secrets_multiple_secrets() {
        let content = r#"
            api_key = "key_123456789012345678901234567890"
            secret = "secret_abcdefghijklmnopqrstuvwxyz"
            password = "my_password_12345678"
        "#;
        let result = redact_secrets(content);
        // All secrets should be redacted
        assert!(result.matches("[REDACTED]").count() >= 2);
    }

    #[test]
    fn test_redact_secrets_no_secrets() {
        let content = r#"
            fn main() {
                let x = 42;
                println!("Hello, world!");
            }
        "#;
        let result = redact_secrets(content);
        assert!(!result.contains("[REDACTED]"));
        assert!(result.contains("Hello, world!"));
    }

    #[test]
    fn test_redact_secrets_github_token() {
        let content = r#"GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"#;
        let result = redact_secrets(content);
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_redact_secrets_jwt_token() {
        let content = "token: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let result = redact_secrets(content);
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_redact_secrets_connection_string() {
        let content = r#"DATABASE_URL = "postgres://user:secret_password@localhost:5432/db""#;
        let result = redact_secrets(content);
        assert!(result.contains("[REDACTED]"));
    }
}
