//! Consolidated MCP Tool Parameters and Handlers
//!
//! This module contains the consolidated tool parameters for the 12 unified MCP tools.

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

