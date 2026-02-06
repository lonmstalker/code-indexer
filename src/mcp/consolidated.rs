//! Consolidated MCP Tool Parameters and Handlers
//!
//! This module contains the consolidated tool parameters for the 12 unified MCP tools,
//! plus the summary-first contract types (get_context_bundle).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// === 1. index_workspace ===
/// Parameters for indexing a workspace
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct IndexWorkspaceParams {
    /// Path to the workspace to index
    #[serde(default)]
    pub path: Option<String>,
    /// Whether to watch for file changes
    #[serde(default)]
    pub watch: Option<bool>,
    /// Include dependencies in indexing
    #[serde(default)]
    pub include_deps: Option<bool>,
}

// === 2. update_files ===
/// Parameters for updating virtual documents
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateFilesParams {
    /// List of files to update
    pub files: Vec<FileUpdate>,
}

/// A single file update
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FileUpdate {
    /// File path
    pub path: String,
    /// File content
    pub content: String,
    /// Version number for conflict detection
    #[serde(default)]
    pub version: Option<u64>,
}

// === 3. list_symbols ===
/// Parameters for listing symbols with filters
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListSymbolsParams {
    /// Filter by symbol kind: "function", "type", "all" (default: "all")
    #[serde(default)]
    pub kind: Option<String>,
    /// Filter by language
    #[serde(default)]
    pub language: Option<String>,
    /// Filter by file path pattern
    #[serde(default)]
    pub file: Option<String>,
    /// Filter by name pattern (glob)
    #[serde(default)]
    pub pattern: Option<String>,
    /// Maximum number of results (default: 100)
    #[serde(default)]
    pub limit: Option<usize>,
    /// Output format: "full", "compact", "minimal" (default: "full")
    #[serde(default)]
    pub format: Option<String>,
    /// Wrap response in ResponseEnvelope (default: false for backward compatibility)
    #[serde(default)]
    pub envelope: Option<bool>,
    /// Pagination cursor (base64-encoded, from previous response's next_cursor)
    #[serde(default)]
    pub cursor: Option<String>,
}

// === 4. search_symbols ===
/// Parameters for searching symbols
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchSymbolsParams {
    /// Search query
    pub query: String,
    /// Filter by symbol kind
    #[serde(default)]
    pub kind: Option<String>,
    /// Filter by language
    #[serde(default)]
    pub language: Option<String>,
    /// Filter by file path pattern
    #[serde(default)]
    pub file: Option<String>,
    /// Filter by module name (for workspace-aware search)
    #[serde(default)]
    pub module: Option<String>,
    /// Enable fuzzy search for typo tolerance
    #[serde(default)]
    pub fuzzy: Option<bool>,
    /// Fuzzy search threshold (0.0-1.0)
    #[serde(default)]
    pub fuzzy_threshold: Option<f64>,
    /// Use regex pattern matching
    #[serde(default)]
    pub regex: Option<bool>,
    /// Maximum number of results (default: 20)
    #[serde(default)]
    pub limit: Option<usize>,
    /// Output format: "full", "compact", "minimal"
    #[serde(default)]
    pub format: Option<String>,
    /// Wrap response in ResponseEnvelope (default: false for backward compatibility)
    #[serde(default)]
    pub envelope: Option<bool>,
    /// Pagination cursor (base64-encoded, from previous response's next_cursor)
    #[serde(default)]
    pub cursor: Option<String>,
    /// Filter by tags (e.g., ["domain:auth", "layer:service"]) - requires ALL tags to match
    #[serde(default)]
    pub tag: Option<Vec<String>>,
    /// Include file metadata (doc1, tags, stability) in results
    #[serde(default)]
    pub include_file_meta: Option<bool>,
    /// Maximum results per directory for diversification
    #[serde(default)]
    pub max_per_directory: Option<usize>,
}

// === 5. get_symbol ===
/// Parameters for getting a symbol by ID or position
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConsolidatedGetSymbolParams {
    /// Symbol ID (for direct lookup)
    #[serde(default)]
    pub id: Option<String>,
    /// List of symbol IDs (for batch lookup)
    #[serde(default)]
    pub ids: Option<Vec<String>>,
    /// File path (for position-based lookup)
    #[serde(default)]
    pub file: Option<String>,
    /// Line number (1-based, for position-based lookup)
    #[serde(default)]
    pub line: Option<u32>,
    /// Column number (for position-based lookup)
    #[serde(default)]
    pub column: Option<u32>,
}

// === 6. find_definitions ===
/// Parameters for finding symbol definitions
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindDefinitionsParams {
    /// Symbol name to find
    pub name: String,
    /// Search in dependencies as well
    #[serde(default)]
    pub include_deps: Option<bool>,
    /// Filter by specific dependency name
    #[serde(default)]
    pub dependency: Option<String>,
}

// === 7. find_references ===
/// Parameters for finding symbol references
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConsolidatedFindReferencesParams {
    /// Symbol name to find references for
    pub name: String,
    /// Include incoming callers (who calls this function)
    #[serde(default)]
    pub include_callers: Option<bool>,
    /// Include files that import this symbol
    #[serde(default)]
    pub include_importers: Option<bool>,
    /// Filter by reference kind: "call", "type_use", "import", "extend"
    #[serde(default)]
    pub kind: Option<String>,
    /// Depth for caller search (default: 1)
    #[serde(default)]
    pub depth: Option<u32>,
    /// Filter by file path pattern
    #[serde(default)]
    pub file: Option<String>,
    /// Maximum number of results
    #[serde(default)]
    pub limit: Option<usize>,
    /// Wrap response in ResponseEnvelope (default: false)
    #[serde(default)]
    pub envelope: Option<bool>,
    /// Pagination cursor (base64-encoded, from previous response's next_cursor)
    #[serde(default)]
    pub cursor: Option<String>,
}

// === 8. analyze_call_graph ===
/// Parameters for analyzing call graph
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnalyzeCallGraphParams {
    /// Entry point function name
    pub function: String,
    /// Direction: "out" (callees), "in" (callers), "both" (default: "out")
    #[serde(default)]
    pub direction: Option<String>,
    /// Maximum depth (default: 3)
    #[serde(default)]
    pub depth: Option<u32>,
    /// Include possible calls (uncertain) in addition to certain calls
    #[serde(default)]
    pub include_possible: Option<bool>,
    /// Filter by confidence: "certain", "possible", "all"
    #[serde(default)]
    pub confidence: Option<String>,
}

// === 9. get_file_outline ===
/// Parameters for getting file structure/outline
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetFileOutlineParams {
    /// File path
    pub file: String,
    /// Start line (for range selection)
    #[serde(default)]
    pub start_line: Option<u32>,
    /// End line (for range selection)
    #[serde(default)]
    pub end_line: Option<u32>,
    /// Include nested scopes
    #[serde(default)]
    pub include_scopes: Option<bool>,
    /// Include file metadata (doc1, purpose, tags, capabilities, staleness)
    #[serde(default)]
    pub include_file_meta: Option<bool>,
}

// === 10. get_imports ===
/// Parameters for getting file imports
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetImportsParams {
    /// File path to get imports for
    pub file: String,
    /// Resolve imports to their definitions
    #[serde(default)]
    pub resolve: Option<bool>,
}

// === 11. get_diagnostics ===
/// Parameters for getting diagnostics
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetDiagnosticsParams {
    /// Type of diagnostics: "dead_code", "all" (default: "all")
    #[serde(default)]
    pub kind: Option<String>,
    /// Filter by file path pattern
    #[serde(default)]
    pub file: Option<String>,
    /// Include metrics in output
    #[serde(default)]
    pub include_metrics: Option<bool>,
    /// Target function or file for metrics
    #[serde(default)]
    pub target: Option<String>,
}

// === 12. get_stats ===
/// Parameters for getting index statistics
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct GetStatsParams {
    /// Get detailed breakdown
    #[serde(default)]
    pub detailed: Option<bool>,
    /// Include workspace/module information
    #[serde(default)]
    pub include_workspace: Option<bool>,
    /// Include dependency information
    #[serde(default)]
    pub include_deps: Option<bool>,
    /// Include architecture summary
    #[serde(default)]
    pub include_architecture: Option<bool>,
}

// === 13. get_snippet ===
/// Parameters for getting code snippets with budget control
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetSnippetParams {
    /// Target: stable_id (e.g. "sid:rust:fn:main@src/main.rs:10") or file:line (e.g. "src/main.rs:10")
    pub target: String,
    /// Number of context lines before and after (default: 3)
    #[serde(default)]
    pub context_lines: Option<usize>,
    /// Maximum total lines to return (default: 50)
    #[serde(default)]
    pub max_lines: Option<usize>,
    /// Expand to full scope boundary (function, struct, etc.)
    #[serde(default)]
    pub expand_to_scope: Option<bool>,
    /// Redact sensitive information (API keys, tokens, passwords). Default: true for agent mode.
    #[serde(default)]
    pub redact: Option<bool>,
}

// =====================================================
// Summary-First Contract: get_context_bundle
// =====================================================

/// Parameters for get_context_bundle - the primary AI-agent entry point
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct GetContextBundleParams {
    /// Input specifying what context to retrieve
    #[serde(default)]
    pub input: Option<ContextInput>,
    /// Budget constraints for the response
    #[serde(default)]
    pub budget: Option<ContextBudget>,
    /// Output format: "full", "compact", "minimal" (default: "minimal")
    #[serde(default)]
    pub format: Option<String>,
    /// Whether to wrap response in envelope (default: true for this tool)
    #[serde(default)]
    pub envelope: Option<bool>,
    /// Optional internal agent routing configuration
    #[serde(default)]
    pub agent: Option<InternalAgentConfig>,
}

/// Input specification for context retrieval
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContextInput {
    /// Search query for finding symbols
    #[serde(default)]
    pub query: Option<String>,
    /// Current file path (for locality-aware results)
    #[serde(default)]
    pub file: Option<String>,
    /// Current position in file (line, column)
    #[serde(default)]
    pub position: Option<ContextPosition>,
    /// Specific symbol IDs to retrieve (batch lookup)
    #[serde(default)]
    pub symbol_ids: Option<Vec<String>>,
    /// Hint about the task (helps prioritize results)
    /// Examples: "refactoring", "debugging", "understanding", "implementing"
    #[serde(default)]
    pub task_hint: Option<String>,
}

/// Position in a file
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContextPosition {
    /// Line number (1-based)
    pub line: u32,
    /// Column number (1-based, optional)
    #[serde(default)]
    pub column: Option<u32>,
}

/// Budget constraints for context response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContextBudget {
    /// Maximum number of items to return (default: 20)
    #[serde(default)]
    pub max_items: Option<usize>,
    /// Maximum response size in bytes
    #[serde(default)]
    pub max_bytes: Option<usize>,
    /// Approximate token budget (for AI context windows)
    #[serde(default)]
    pub approx_tokens: Option<usize>,
    /// Number of snippet lines to include (0 = no snippets, default: 3)
    #[serde(default)]
    pub snippet_lines: Option<usize>,
    /// Include code snippets in response
    #[serde(default)]
    pub include_snippets: Option<bool>,
    /// Sample size when truncating (default: 5)
    #[serde(default)]
    pub sample_k: Option<usize>,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_items: Some(20),
            max_bytes: None,
            approx_tokens: None,
            snippet_lines: Some(3),
            include_snippets: Some(false),
            sample_k: Some(5),
        }
    }
}

/// Internal LLM agent routing configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InternalAgentConfig {
    /// Provider name: openai, anthropic, openrouter, local, none
    #[serde(default)]
    pub provider: Option<String>,
    /// Model ID, provider-specific
    #[serde(default)]
    pub model: Option<String>,
    /// Custom endpoint/base URL (optional)
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Raw API key/token for provider auth (prefer api_key_env in config)
    #[serde(default)]
    pub api_key: Option<String>,
    /// Env var name with API key/token (e.g. OPENAI_API_KEY)
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Agent mode: planner, reranker, synthesizer (default: planner)
    #[serde(default)]
    pub mode: Option<String>,
}

/// Agent-friendly context API (single-query input) for Codex/Claude-like clients
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PrepareContextParams {
    /// Natural language query from external coding agent
    pub query: String,
    /// Current file path for locality-aware ranking
    #[serde(default)]
    pub file: Option<String>,
    /// Cursor line in current file
    #[serde(default)]
    pub line: Option<u32>,
    /// Cursor column in current file
    #[serde(default)]
    pub column: Option<u32>,
    /// Task intent hint (e.g. refactoring/debugging/understanding/implementing)
    #[serde(default)]
    pub task_hint: Option<String>,
    /// Maximum number of ranked symbols to return
    #[serde(default)]
    pub max_items: Option<usize>,
    /// Approximate token budget for the resulting context package
    #[serde(default)]
    pub approx_tokens: Option<usize>,
    /// Include code snippets in context cards
    #[serde(default)]
    pub include_snippets: Option<bool>,
    /// Snippet lines to include around symbol start line
    #[serde(default)]
    pub snippet_lines: Option<usize>,
    /// Response format: full, compact, minimal
    #[serde(default)]
    pub format: Option<String>,
    /// Whether to wrap in envelope (defaults to true)
    #[serde(default)]
    pub envelope: Option<bool>,
    /// Optional internal agent routing configuration
    #[serde(default)]
    pub agent: Option<InternalAgentConfig>,
    /// Agent orchestration timeout in milliseconds
    #[serde(default)]
    pub agent_timeout_ms: Option<u64>,
    /// Maximum number of agent orchestration steps
    #[serde(default)]
    pub agent_max_steps: Option<u32>,
    /// Include per-step collection trace (debug only)
    #[serde(default)]
    pub include_trace: Option<bool>,
}

impl From<PrepareContextParams> for GetContextBundleParams {
    fn from(value: PrepareContextParams) -> Self {
        let position = value.line.map(|line| ContextPosition {
            line,
            column: value.column,
        });

        let budget = ContextBudget {
            max_items: value.max_items,
            max_bytes: None,
            approx_tokens: value.approx_tokens,
            snippet_lines: value.snippet_lines,
            include_snippets: value.include_snippets,
            sample_k: Some(5),
        };

        Self {
            input: Some(ContextInput {
                query: Some(value.query),
                file: value.file,
                position,
                symbol_ids: None,
                task_hint: value.task_hint,
            }),
            budget: Some(budget),
            format: value.format,
            envelope: Some(value.envelope.unwrap_or(true)),
            agent: value.agent,
        }
    }
}

// === Context Bundle Output Types ===

/// A symbol card for compact representation
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolCard {
    /// Stable identifier
    pub id: String,
    /// Fully qualified domain name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fqdn: Option<String>,
    /// Symbol kind
    pub kind: String,
    /// Signature (for functions/methods)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
    /// Location (file:line)
    pub loc: String,
    /// Relevance rank (1 = most relevant)
    pub rank: u32,
    /// Code snippet (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// A usage reference (diversified: 1-2 per file/scope)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsageRef {
    /// File path
    pub file: String,
    /// Line number
    pub line: u32,
    /// Context snippet
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Reference kind
    pub kind: String,
}

/// Call neighborhood (incoming/outgoing calls)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CallNeighborhood {
    /// Incoming calls (who calls this)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callers: Vec<CallRef>,
    /// Outgoing calls (what this calls)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callees: Vec<CallRef>,
}

/// A call reference
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CallRef {
    /// Symbol name
    pub name: String,
    /// Symbol ID (if resolved)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Location
    pub loc: String,
    /// Confidence: "certain" or "possible"
    #[serde(default = "default_confidence_str")]
    pub confidence: String,
}

fn default_confidence_str() -> String {
    "certain".to_string()
}

/// Relevant import
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RelevantImport {
    /// Imported path/module
    pub path: String,
    /// Specific symbol (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Source file
    pub from_file: String,
}

/// Full context bundle response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContextBundle {
    /// Symbol cards (primary results)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbol_cards: Vec<SymbolCard>,
    /// Top usages (diversified)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_usages: Vec<UsageRef>,
    /// Call neighborhood
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_neighborhood: Option<CallNeighborhood>,
    /// Relevant imports
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports_relevant: Vec<RelevantImport>,
    /// Selected internal agent backend details (if configured)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<ContextAgentInfo>,
    /// Recommended follow-up tool calls for orchestrating agents
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_tool_calls: Vec<SuggestedToolCall>,
    /// Unified task-centric context digest collected by agent orchestration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_context: Option<TaskContextDigest>,
    /// Coverage status for context layers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<ContextCoverage>,
    /// Explicit collection gaps (never silently truncated)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<CoverageGap>,
    /// Agent collection metadata and optional trace
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_meta: Option<AgentCollectionMeta>,
}

/// Internal agent details reflected in context bundle
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContextAgentInfo {
    /// Provider name
    pub provider: String,
    /// Model name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Endpoint/base URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Whether auth token is configured (token value is never exposed)
    pub auth_configured: bool,
    /// Auth source hint: inline or env:<VAR_NAME>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_source: Option<String>,
    /// Agent mode
    pub mode: String,
}

/// Suggested follow-up tool call for context expansion
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SuggestedToolCall {
    /// Tool name to call next
    pub tool: String,
    /// Recommended arguments
    pub args: serde_json::Value,
    /// Why this call is recommended
    pub reason: String,
}

/// Structured task-level context digest collected by the agent orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct TaskContextDigest {
    /// Module-to-module dependency edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub module_graph: Vec<ModuleDependencyEdge>,
    /// File-level import graph edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_import_graph: Vec<FileImportEdge>,
    /// Symbol-level interaction graph edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbol_interactions: Vec<SymbolInteractionEdge>,
    /// Dependency API touchpoints discovered for the task.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps_touchpoints: Vec<DependencyTouchpoint>,
    /// Documentation/config/architecture digest entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub docs_config_digest: Vec<DocConfigDigestEntry>,
}

/// Module dependency edge.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleDependencyEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
}

/// File import edge.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileImportEdge {
    pub from_file: String,
    pub imported_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_symbol: Option<String>,
}

/// Symbol interaction edge.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolInteractionEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

/// Dependency touchpoint.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DependencyTouchpoint {
    pub dependency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Docs/config digest entry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocConfigDigestEntry {
    pub source: String,
    pub summary: String,
}

/// Coverage status for required/optional layers.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ContextCoverage {
    pub module_graph: bool,
    pub file_import_graph: bool,
    pub symbol_interaction_graph: bool,
    pub deps_touchpoints: bool,
    pub docs_config_digest: bool,
    pub complete: bool,
}

/// Explicitly reported gap in context coverage.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CoverageGap {
    pub layer: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_tool_call: Option<SuggestedToolCall>,
}

/// Aggregated metadata for agent context collection.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentCollectionMeta {
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    pub steps_taken: u32,
    pub elapsed_ms: u64,
    pub timeout_reached: bool,
    pub max_steps_reached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<AgentTokenUsage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<AgentTraceStep>,
}

/// Token usage reported by the upstream chat-completions endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentTokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// Per-step trace entry for debugging orchestration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentTraceStep {
    pub step: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<AgentTraceCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Trace call entry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentTraceCall {
    pub tool: String,
    pub args: serde_json::Value,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// =====================================================
// P0-5: Doc/Config Digest Types
// =====================================================

/// Parameters for getting a documentation section
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetDocSectionParams {
    /// File path (e.g., "README.md") or doc type ("readme", "contributing")
    pub target: String,
    /// Optional section heading to extract (e.g., "Installation", "Usage")
    #[serde(default)]
    pub section: Option<String>,
    /// Include code blocks from the section
    #[serde(default)]
    pub include_code: Option<bool>,
}

/// Response for get_doc_section
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocSectionResponse {
    /// File path
    pub file_path: String,
    /// Document type
    pub doc_type: String,
    /// Document title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// All heading names (for discovery)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_sections: Vec<String>,
    /// Extracted section content (if section was requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section_content: Option<String>,
    /// Code blocks in the section (if include_code=true)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_blocks: Vec<DocCodeBlock>,
}

/// A code block from documentation
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocCodeBlock {
    /// Language hint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Code content
    pub content: String,
    /// Line number in original file
    pub line: u32,
}

/// Parameters for getting project commands
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct GetProjectCommandsParams {
    /// Filter by command type: "run", "build", "test", "all" (default: "all")
    #[serde(default)]
    pub kind: Option<String>,
}

/// Response for project commands
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProjectCommandsResponse {
    /// Run/start commands
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run: Vec<String>,
    /// Build commands
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build: Vec<String>,
    /// Test commands
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test: Vec<String>,
}

// =====================================================
// P0-1: Project Compass Types
// =====================================================

/// Parameters for get_project_compass - macro-level project overview
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct GetProjectCompassParams {
    /// Project path (defaults to indexed workspace)
    #[serde(default)]
    pub path: Option<String>,
    /// Maximum response size in bytes (default: 16KB)
    #[serde(default)]
    pub max_bytes: Option<usize>,
    /// Include entry points (default: true)
    #[serde(default)]
    pub include_entry_points: Option<bool>,
    /// Include module hierarchy (default: true)
    #[serde(default)]
    pub include_modules: Option<bool>,
    /// Include documentation info (default: true)
    #[serde(default)]
    pub include_docs: Option<bool>,
}

/// Response for get_project_compass
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProjectCompassResponse {
    /// Response metadata
    pub meta: CompassMeta,
    /// Project profile (languages, frameworks, build tools)
    pub profile: CompassProfile,
    /// Available commands (run, build, test)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<CompassCommands>,
    /// Detected entry points
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entry_points: Vec<CompassEntryPoint>,
    /// Top-level modules/directories
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules_top: Vec<CompassModuleNode>,
    /// Documentation info
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<CompassDocs>,
    /// Suggested next actions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next: Vec<CompassNextAction>,
}

/// Metadata for compass response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompassMeta {
    /// Database revision
    pub db_rev: u64,
    /// Profile revision
    pub profile_rev: u64,
    /// Response size info
    pub budget: CompassBudget,
}

/// Budget info for compass response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompassBudget {
    /// Actual response bytes
    pub actual_bytes: usize,
    /// Maximum allowed bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
}

/// Project profile summary
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompassProfile {
    /// Language statistics
    pub languages: Vec<CompassLanguage>,
    /// Detected frameworks
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frameworks: Vec<String>,
    /// Build tools
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_tools: Vec<String>,
    /// Workspace type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_type: Option<String>,
}

/// Language statistics
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompassLanguage {
    /// Language name
    pub name: String,
    /// Number of files
    pub files: usize,
    /// Number of symbols
    pub symbols: usize,
    /// Percentage of codebase
    pub pct: f32,
}

/// Project commands
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompassCommands {
    /// Run commands
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run: Vec<String>,
    /// Build commands
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build: Vec<String>,
    /// Test commands
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test: Vec<String>,
}

/// Entry point info
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompassEntryPoint {
    /// Entry point name
    pub name: String,
    /// Entry type (main, server, cli, etc.)
    pub entry_type: String,
    /// File path
    pub file: String,
    /// Line number
    pub line: u32,
    /// Evidence for detection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// Module node for hierarchy
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompassModuleNode {
    /// Node ID (e.g., "mod:src/api")
    pub id: String,
    /// Node type (module, directory, layer, package)
    pub node_type: String,
    /// Display name
    pub name: String,
    /// Path
    pub path: String,
    /// Symbol count
    pub symbol_count: usize,
    /// File count
    #[serde(default)]
    pub file_count: usize,
}

/// Documentation info
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompassDocs {
    /// README headings (top-level)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub readme_headings: Vec<String>,
    /// Has CONTRIBUTING.md
    #[serde(default)]
    pub has_contributing: bool,
    /// Has CHANGELOG.md
    #[serde(default)]
    pub has_changelog: bool,
}

/// Suggested next action
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompassNextAction {
    /// Tool to call
    pub tool: String,
    /// Arguments
    pub args: serde_json::Value,
    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// =====================================================
// P0-2: Expand Project Node Types
// =====================================================

/// Parameters for expand_project_node - drill-down into modules
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ExpandProjectNodeParams {
    /// Node ID to expand (e.g., "mod:src/api" or "dir:src")
    pub node_id: String,
    /// Maximum number of items (default: 20)
    #[serde(default)]
    pub limit: Option<usize>,
    /// Include top symbols from this node
    #[serde(default)]
    pub include_symbols: Option<bool>,
    /// Pagination cursor
    #[serde(default)]
    pub cursor: Option<String>,
}

/// Response for expand_project_node
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExpandedNodeResponse {
    /// The expanded node
    pub node: CompassModuleNode,
    /// Child nodes
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<CompassModuleNode>,
    /// Top files in this node
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_files: Vec<NodeFileInfo>,
    /// Top symbols in this node (if include_symbols=true)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_symbols: Vec<SymbolCard>,
    /// Pagination cursor for more results
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// File info within a node
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NodeFileInfo {
    /// File path
    pub path: String,
    /// Language
    pub language: String,
    /// Symbol count
    pub symbol_count: usize,
}

// =====================================================
// P0-3: Get Compass (Query) Types
// =====================================================

/// Parameters for get_compass - task-oriented diversified search
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetCompassQueryParams {
    /// Search query (e.g., "auth", "login", "database connection")
    pub query: String,
    /// Current file path for locality boost
    #[serde(default)]
    pub current_file: Option<String>,
    /// Maximum number of results (default: 10)
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Response for get_compass (query-based)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompassQueryResponse {
    /// Query that was executed
    pub query: String,
    /// Diversified results
    pub results: Vec<CompassResult>,
    /// Total matches found (before diversification)
    pub total_matches: usize,
}

/// A single compass search result
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompassResult {
    /// Result type: "module", "file", "symbol", "doc", "command"
    pub result_type: String,
    /// Name
    pub name: String,
    /// Path or location
    pub path: String,
    /// Why this result was included
    pub why: String,
    /// Relevance score (0-1)
    pub score: f32,
    /// Symbol ID (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,
    /// Line number (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

// =====================================================
// P0-4: Session Dictionary Codec Types
// =====================================================

/// Parameters for open_session
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct OpenSessionParams {
    /// Restore an existing session by ID
    #[serde(default)]
    pub restore_session: Option<String>,
    /// Project path for session context
    #[serde(default)]
    pub project_path: Option<String>,
}

/// Response for open_session
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionResponse {
    /// Session ID (use this for subsequent requests)
    pub session_id: String,
    /// Current dictionary state
    pub dict: SessionDict,
    /// Whether this is a restored session
    pub restored: bool,
}

/// Session dictionary state
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SessionDict {
    /// File path mappings (id -> path)
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub files: std::collections::HashMap<u32, String>,
    /// Symbol kind mappings (id -> kind)
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub kinds: std::collections::HashMap<u8, String>,
    /// Module mappings (id -> module)
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub modules: std::collections::HashMap<u16, String>,
}

/// Parameters for close_session
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CloseSessionParams {
    /// Session ID to close
    pub session_id: String,
}

/// Response for close_session
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CloseSessionResponse {
    /// Whether the session was successfully closed
    pub closed: bool,
}

// =====================================================
// Tag Management Types
// =====================================================

/// Parameters for manage_tags tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ManageTagsParams {
    /// Action to perform: "add_rule", "remove_rule", "list_rules", "preview", "apply", "stats"
    pub action: String,
    /// Glob pattern (for add_rule/remove_rule)
    #[serde(default)]
    pub pattern: Option<String>,
    /// Tags to add (for add_rule)
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Confidence score (for add_rule, default: 0.7)
    #[serde(default)]
    pub confidence: Option<f64>,
    /// File path (for preview action)
    #[serde(default)]
    pub file: Option<String>,
    /// Project path (defaults to current working directory)
    #[serde(default)]
    pub path: Option<String>,
}

/// Response for manage_tags
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ManageTagsResponse {
    /// Whether the action was successful
    pub success: bool,
    /// Message describing the result
    pub message: String,
    /// List of rules (for list_rules action)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<TagRuleInfo>,
    /// Preview results (for preview action)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preview: Vec<TagPreviewResult>,
    /// Stats (for stats action)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stats: Vec<TagStatInfo>,
    /// Warnings (e.g., unknown tags)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Information about a tag rule
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TagRuleInfo {
    /// Glob pattern
    pub pattern: String,
    /// Tags that will be applied
    pub tags: Vec<String>,
    /// Confidence score
    pub confidence: f64,
}

/// Preview result for a single matching rule
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TagPreviewResult {
    /// Pattern that matched
    pub pattern: String,
    /// Tags that would be applied
    pub tags: Vec<String>,
    /// Confidence score
    pub confidence: f64,
}

// =====================================================
// Indexing Progress Types
// =====================================================

/// Parameters for get_indexing_status (no parameters needed)
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct GetIndexingStatusParams {}

/// Tag statistics entry
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TagStatInfo {
    /// Tag category
    pub category: String,
    /// Tag name
    pub tag: String,
    /// Number of files with this tag
    pub count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_context_converts_to_context_bundle_params() {
        let params = PrepareContextParams {
            query: "find user auth flow".to_string(),
            file: Some("src/auth/service.rs".to_string()),
            line: Some(42),
            column: Some(7),
            task_hint: Some("debugging".to_string()),
            max_items: Some(15),
            approx_tokens: Some(4096),
            include_snippets: Some(true),
            snippet_lines: Some(5),
            format: Some("minimal".to_string()),
            envelope: Some(true),
            agent: Some(InternalAgentConfig {
                provider: Some("openai".to_string()),
                model: Some("gpt-4o-mini".to_string()),
                endpoint: None,
                api_key: None,
                api_key_env: Some("OPENAI_API_KEY".to_string()),
                mode: Some("planner".to_string()),
            }),
            agent_timeout_ms: Some(120_000),
            agent_max_steps: Some(8),
            include_trace: Some(true),
        };

        let converted: GetContextBundleParams = params.into();
        let input = converted.input.expect("input");
        let budget = converted.budget.expect("budget");

        assert_eq!(input.query.as_deref(), Some("find user auth flow"));
        assert_eq!(input.file.as_deref(), Some("src/auth/service.rs"));
        assert_eq!(input.position.expect("position").line, 42);
        assert_eq!(budget.max_items, Some(15));
        assert_eq!(budget.approx_tokens, Some(4096));
        assert_eq!(budget.include_snippets, Some(true));
        assert_eq!(converted.format.as_deref(), Some("minimal"));
        assert!(converted.agent.is_some());
    }
}

