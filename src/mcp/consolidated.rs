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
}

