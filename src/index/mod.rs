pub mod migrations;
pub mod models;
pub mod overlay;
pub mod sqlite;
pub mod write_queue;

use crate::error::Result;
pub use models::*;
pub use overlay::DocumentOverlay;
pub use write_queue::WriteQueueHandle;

pub trait CodeIndex: Send + Sync {
    #[allow(dead_code)]
    fn add_symbol(&self, symbol: Symbol) -> Result<()>;
    fn add_symbols(&self, symbols: Vec<Symbol>) -> Result<()>;
    fn remove_file(&self, file_path: &str) -> Result<()>;
    fn get_symbol(&self, id: &str) -> Result<Option<Symbol>>;
    fn search(&self, query: &str, options: &SearchOptions) -> Result<Vec<SearchResult>>;
    /// Fuzzy search with typo tolerance using Jaro-Winkler similarity
    fn search_fuzzy(&self, query: &str, options: &SearchOptions) -> Result<Vec<SearchResult>>;
    fn find_definition(&self, name: &str) -> Result<Vec<Symbol>>;
    /// Find definitions with optional parent type filter for type-aware resolution.
    /// This allows resolving `obj.method()` to the correct `Type::method` instead of
    /// finding all methods named "method".
    fn find_definition_by_parent(
        &self,
        name: &str,
        parent_type: Option<&str>,
        language: Option<&str>,
    ) -> Result<Vec<Symbol>>;
    fn list_functions(&self, options: &SearchOptions) -> Result<Vec<Symbol>>;
    fn list_types(&self, options: &SearchOptions) -> Result<Vec<Symbol>>;
    fn get_file_symbols(&self, file_path: &str) -> Result<Vec<Symbol>>;
    fn get_stats(&self) -> Result<IndexStats>;
    fn clear(&self) -> Result<()>;

    // Reference tracking methods
    fn add_references(&self, references: Vec<SymbolReference>) -> Result<()>;
    fn find_references(&self, symbol_name: &str, options: &SearchOptions) -> Result<Vec<SymbolReference>>;
    fn find_callers(&self, function_name: &str, depth: Option<u32>) -> Result<Vec<SymbolReference>>;
    fn find_implementations(&self, trait_name: &str) -> Result<Vec<Symbol>>;
    fn get_symbol_members(&self, type_name: &str) -> Result<Vec<Symbol>>;

    // Import tracking methods
    fn add_imports(&self, imports: Vec<FileImport>) -> Result<()>;
    fn get_file_imports(&self, file_path: &str) -> Result<Vec<FileImport>>;
    fn get_file_importers(&self, file_path: &str) -> Result<Vec<String>>;

    // Call graph and analysis methods
    /// Find all functions called by the given function
    fn find_callees(&self, function_name: &str) -> Result<Vec<SymbolReference>>;

    /// Build a call graph starting from an entry point with maximum depth
    fn get_call_graph(&self, entry_point: &str, max_depth: u32) -> Result<CallGraph>;

    /// Find unused (dead) code - functions and types without references
    fn find_dead_code(&self) -> Result<DeadCodeReport>;

    /// Get metrics for a specific function
    fn get_function_metrics(&self, function_name: &str) -> Result<Vec<FunctionMetrics>>;

    /// Get metrics for all functions in a file
    fn get_file_metrics(&self, file_path: &str) -> Result<Vec<FunctionMetrics>>;

    // Documentation and configuration digest methods
    /// Get all configuration digests (package.json, Cargo.toml, etc.)
    fn get_all_config_digests(&self) -> Result<Vec<crate::docs::ConfigDigest>>;

    /// Get list of all indexed file paths
    fn get_indexed_files(&self) -> Result<Vec<String>>;
}
