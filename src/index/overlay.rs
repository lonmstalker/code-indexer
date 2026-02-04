//! Document Overlay for virtual/dirty documents
//!
//! This module provides a way to track uncommitted changes to documents
//! before they are persisted to the SQLite index.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::index::{Scope, Symbol};

/// A virtual document representing an in-memory modification
#[derive(Debug, Clone)]
pub struct VirtualDocument {
    /// The content of the document
    pub content: String,
    /// Version number for change tracking
    pub version: u64,
    /// Symbols extracted from the document
    pub symbols: Vec<Symbol>,
    /// Scopes extracted from the document
    pub scopes: Vec<Scope>,
    /// Whether the document has been modified since last commit
    pub dirty: bool,
}

impl VirtualDocument {
    /// Creates a new virtual document
    pub fn new(content: String, version: u64) -> Self {
        Self {
            content,
            version,
            symbols: Vec::new(),
            scopes: Vec::new(),
            dirty: true,
        }
    }

    /// Updates the document content and increments version
    pub fn update(&mut self, content: String) {
        self.content = content;
        self.version += 1;
        self.dirty = true;
        // Clear cached data - will be re-extracted
        self.symbols.clear();
        self.scopes.clear();
    }

    /// Sets the extracted symbols
    pub fn set_symbols(&mut self, symbols: Vec<Symbol>) {
        self.symbols = symbols;
    }

    /// Sets the extracted scopes
    pub fn set_scopes(&mut self, scopes: Vec<Scope>) {
        self.scopes = scopes;
    }

    /// Marks the document as clean (committed)
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }
}

/// Document overlay that tracks virtual/dirty documents
///
/// This allows the indexer to handle unsaved changes in editors
/// and provide accurate completions/references for modified files.
#[derive(Debug, Default)]
pub struct DocumentOverlay {
    /// Map of file path to virtual document
    documents: RwLock<HashMap<String, VirtualDocument>>,
}

impl DocumentOverlay {
    /// Creates a new empty overlay
    pub fn new() -> Self {
        Self {
            documents: RwLock::new(HashMap::new()),
        }
    }

    /// Updates or creates a virtual document
    pub fn update(&self, path: &str, content: &str, version: u64) {
        let mut docs = self.documents.write().unwrap();
        if let Some(doc) = docs.get_mut(path) {
            if version > doc.version {
                doc.update(content.to_string());
                doc.version = version;
            }
        } else {
            docs.insert(path.to_string(), VirtualDocument::new(content.to_string(), version));
        }
    }

    /// Gets a virtual document if it exists
    pub fn get(&self, path: &str) -> Option<VirtualDocument> {
        let docs = self.documents.read().unwrap();
        docs.get(path).cloned()
    }

    /// Checks if a path has a virtual document
    pub fn contains(&self, path: &str) -> bool {
        let docs = self.documents.read().unwrap();
        docs.contains_key(path)
    }

    /// Gets symbols from a virtual document
    pub fn get_symbols(&self, path: &str) -> Option<Vec<Symbol>> {
        let docs = self.documents.read().unwrap();
        docs.get(path).map(|d| d.symbols.clone())
    }

    /// Gets scopes from a virtual document
    pub fn get_scopes(&self, path: &str) -> Option<Vec<Scope>> {
        let docs = self.documents.read().unwrap();
        docs.get(path).map(|d| d.scopes.clone())
    }

    /// Sets symbols for a virtual document
    pub fn set_symbols(&self, path: &str, symbols: Vec<Symbol>) {
        let mut docs = self.documents.write().unwrap();
        if let Some(doc) = docs.get_mut(path) {
            doc.set_symbols(symbols);
        }
    }

    /// Sets scopes for a virtual document
    pub fn set_scopes(&self, path: &str, scopes: Vec<Scope>) {
        let mut docs = self.documents.write().unwrap();
        if let Some(doc) = docs.get_mut(path) {
            doc.set_scopes(scopes);
        }
    }

    /// Removes a virtual document (discards changes)
    pub fn discard(&self, path: &str) {
        let mut docs = self.documents.write().unwrap();
        docs.remove(path);
    }

    /// Marks a document as committed (clean)
    pub fn mark_committed(&self, path: &str) {
        let mut docs = self.documents.write().unwrap();
        if let Some(doc) = docs.get_mut(path) {
            doc.mark_clean();
        }
    }

    /// Gets all dirty document paths
    pub fn dirty_paths(&self) -> Vec<String> {
        let docs = self.documents.read().unwrap();
        docs.iter()
            .filter(|(_, doc)| doc.dirty)
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// Gets all virtual document paths
    pub fn all_paths(&self) -> Vec<String> {
        let docs = self.documents.read().unwrap();
        docs.keys().cloned().collect()
    }

    /// Clears all virtual documents
    pub fn clear(&self) {
        let mut docs = self.documents.write().unwrap();
        docs.clear();
    }

    /// Gets the version of a virtual document
    pub fn get_version(&self, path: &str) -> Option<u64> {
        let docs = self.documents.read().unwrap();
        docs.get(path).map(|d| d.version)
    }

    /// Checks if a document is dirty
    pub fn is_dirty(&self, path: &str) -> bool {
        let docs = self.documents.read().unwrap();
        docs.get(path).map(|d| d.dirty).unwrap_or(false)
    }

    /// Gets the overlay revision info for the Summary-First Contract
    ///
    /// Returns the count of dirty files and the maximum version number
    /// across all overlay documents.
    pub fn get_overlay_revision(&self) -> crate::index::OverlayRevision {
        let docs = self.documents.read().unwrap();
        let dirty_files = docs.values().filter(|d| d.dirty).count();
        let max_version = docs.values().map(|d| d.version).max().unwrap_or(0);
        crate::index::OverlayRevision {
            dirty_files,
            max_version,
        }
    }

    // === Summary-First Contract: Overlay-Priority Methods ===

    /// Search symbols with overlay priority
    ///
    /// 1. First searches in overlay documents
    /// 2. Then searches in DB excluding overlay file paths
    /// 3. Merges results with overlay symbols having priority
    pub fn search_with_overlay(
        &self,
        query: &str,
        db_index: &crate::index::sqlite::SqliteIndex,
        options: &crate::index::SearchOptions,
    ) -> crate::error::Result<Vec<crate::index::SearchResult>> {
        let query_lower = query.to_lowercase();
        let limit = options.limit.unwrap_or(20);

        // Collect overlay file paths
        let overlay_paths = self.all_paths();

        // Search in overlay symbols
        let mut overlay_results: Vec<crate::index::SearchResult> = {
            let docs = self.documents.read().unwrap();
            docs.values()
                .flat_map(|doc| {
                    doc.symbols.iter().filter_map(|sym| {
                        // Simple name matching for overlay
                        let name_lower = sym.name.to_lowercase();
                        if name_lower.contains(&query_lower) || name_lower.starts_with(&query_lower)
                        {
                            // Calculate simple score based on match quality
                            let score = if name_lower == query_lower {
                                1.0
                            } else if name_lower.starts_with(&query_lower) {
                                0.9
                            } else {
                                0.7
                            };
                            Some(crate::index::SearchResult {
                                symbol: sym.clone(),
                                score,
                            })
                        } else {
                            None
                        }
                    })
                })
                .collect()
        };

        // Sort overlay results by score
        overlay_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // If we have enough from overlay, return early
        if overlay_results.len() >= limit {
            overlay_results.truncate(limit);
            return Ok(overlay_results);
        }

        // Search DB excluding overlay files
        let remaining_limit = limit - overlay_results.len();
        let mut db_options = options.clone();
        db_options.limit = Some(remaining_limit);

        let db_results = db_index.search_excluding_files(query, &db_options, &overlay_paths)?;

        // Merge results: overlay first, then DB
        overlay_results.extend(db_results);
        overlay_results.truncate(limit);

        Ok(overlay_results)
    }

    /// Get symbol with overlay priority
    ///
    /// Checks overlay first, falls back to database if not found
    pub fn get_symbol_with_overlay(
        &self,
        symbol_id: &str,
        db_index: &crate::index::sqlite::SqliteIndex,
    ) -> crate::error::Result<Option<Symbol>> {
        // First check overlay documents for a symbol with this ID
        {
            let docs = self.documents.read().unwrap();
            for doc in docs.values() {
                if let Some(sym) = doc.symbols.iter().find(|s| s.id == symbol_id) {
                    return Ok(Some(sym.clone()));
                }
            }
        }

        // Fall back to database
        use crate::index::CodeIndex;
        db_index.get_symbol(symbol_id)
    }

    /// Get symbol at a specific file position with overlay priority
    pub fn get_symbol_at_position(
        &self,
        file_path: &str,
        line: u32,
        column: u32,
    ) -> Option<Symbol> {
        let docs = self.documents.read().unwrap();
        if let Some(doc) = docs.get(file_path) {
            // Find symbol that contains this position
            doc.symbols.iter().find(|s| {
                s.location.file_path == file_path
                    && s.location.start_line <= line
                    && s.location.end_line >= line
                    && (s.location.start_line != line || s.location.start_column <= column)
                    && (s.location.end_line != line || s.location.end_column >= column)
            }).cloned()
        } else {
            None
        }
    }

    /// Get all overlay symbols matching a name pattern
    pub fn find_symbols_by_name(&self, name: &str) -> Vec<Symbol> {
        let name_lower = name.to_lowercase();
        let docs = self.documents.read().unwrap();
        docs.values()
            .flat_map(|doc| {
                doc.symbols.iter().filter(|s| {
                    s.name.to_lowercase() == name_lower
                }).cloned()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{Location, Scope, ScopeKind, Symbol, SymbolKind};

    // === Helper functions ===

    fn create_test_symbol(name: &str, file: &str, start_line: u32, end_line: u32) -> Symbol {
        Symbol::new(
            name,
            SymbolKind::Function,
            Location::new(file, start_line, 0, end_line, 0),
            "rust",
        )
    }

    // === Basic overlay tests ===

    #[test]
    fn test_overlay_update() {
        let overlay = DocumentOverlay::new();

        overlay.update("test.rs", "fn main() {}", 1);
        assert!(overlay.contains("test.rs"));

        let doc = overlay.get("test.rs").unwrap();
        assert_eq!(doc.content, "fn main() {}");
        assert_eq!(doc.version, 1);
        assert!(doc.dirty);
    }

    #[test]
    fn test_overlay_version_ordering() {
        let overlay = DocumentOverlay::new();

        overlay.update("test.rs", "v1", 1);
        overlay.update("test.rs", "v2", 2);

        let doc = overlay.get("test.rs").unwrap();
        assert_eq!(doc.content, "v2");
        assert_eq!(doc.version, 2);

        // Older version should not update
        overlay.update("test.rs", "v0", 1);
        let doc = overlay.get("test.rs").unwrap();
        assert_eq!(doc.content, "v2");
    }

    #[test]
    fn test_overlay_discard() {
        let overlay = DocumentOverlay::new();

        overlay.update("test.rs", "content", 1);
        assert!(overlay.contains("test.rs"));

        overlay.discard("test.rs");
        assert!(!overlay.contains("test.rs"));
    }

    #[test]
    fn test_overlay_dirty_tracking() {
        let overlay = DocumentOverlay::new();

        overlay.update("test.rs", "content", 1);
        assert!(overlay.is_dirty("test.rs"));
        assert_eq!(overlay.dirty_paths(), vec!["test.rs"]);

        overlay.mark_committed("test.rs");
        assert!(!overlay.is_dirty("test.rs"));
        assert!(overlay.dirty_paths().is_empty());
    }

    // === get_overlay_revision tests ===

    #[test]
    fn test_get_overlay_revision_empty() {
        let overlay = DocumentOverlay::new();
        let rev = overlay.get_overlay_revision();
        assert_eq!(rev.dirty_files, 0);
        assert_eq!(rev.max_version, 0);
    }

    #[test]
    fn test_get_overlay_revision_single_dirty() {
        let overlay = DocumentOverlay::new();
        overlay.update("test.rs", "content", 5);

        let rev = overlay.get_overlay_revision();
        assert_eq!(rev.dirty_files, 1);
        assert_eq!(rev.max_version, 5);
    }

    #[test]
    fn test_get_overlay_revision_multiple_dirty() {
        let overlay = DocumentOverlay::new();
        overlay.update("a.rs", "content a", 3);
        overlay.update("b.rs", "content b", 7);
        overlay.update("c.rs", "content c", 2);

        let rev = overlay.get_overlay_revision();
        assert_eq!(rev.dirty_files, 3);
        assert_eq!(rev.max_version, 7);
    }

    #[test]
    fn test_get_overlay_revision_mixed_dirty_clean() {
        let overlay = DocumentOverlay::new();
        overlay.update("dirty1.rs", "content", 3);
        overlay.update("dirty2.rs", "content", 5);
        overlay.update("clean.rs", "content", 4);
        overlay.mark_committed("clean.rs");

        let rev = overlay.get_overlay_revision();
        assert_eq!(rev.dirty_files, 2);
        assert_eq!(rev.max_version, 5); // max_version includes clean documents
    }

    #[test]
    fn test_get_overlay_revision_max_version_tracking() {
        let overlay = DocumentOverlay::new();
        overlay.update("a.rs", "v1", 1);
        overlay.update("a.rs", "v10", 10);
        overlay.update("b.rs", "v5", 5);

        let rev = overlay.get_overlay_revision();
        assert_eq!(rev.max_version, 10);
    }

    // === get_symbol_at_position tests ===

    #[test]
    fn test_get_symbol_at_position_exact_match() {
        let overlay = DocumentOverlay::new();
        overlay.update("test.rs", "fn foo() {}", 1);

        let symbol = create_test_symbol("foo", "test.rs", 1, 3);
        overlay.set_symbols("test.rs", vec![symbol]);

        let found = overlay.get_symbol_at_position("test.rs", 2, 5);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "foo");
    }

    #[test]
    fn test_get_symbol_at_position_inside_symbol() {
        let overlay = DocumentOverlay::new();
        overlay.update("test.rs", "fn foo() { let x = 1; }", 1);

        let symbol = create_test_symbol("foo", "test.rs", 1, 10);
        overlay.set_symbols("test.rs", vec![symbol]);

        // Position inside the symbol
        let found = overlay.get_symbol_at_position("test.rs", 5, 0);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "foo");
    }

    #[test]
    fn test_get_symbol_at_position_no_match() {
        let overlay = DocumentOverlay::new();
        overlay.update("test.rs", "fn foo() {}", 1);

        let symbol = create_test_symbol("foo", "test.rs", 1, 3);
        overlay.set_symbols("test.rs", vec![symbol]);

        // Position outside the symbol
        let found = overlay.get_symbol_at_position("test.rs", 50, 0);
        assert!(found.is_none());
    }

    #[test]
    fn test_get_symbol_at_position_file_not_in_overlay() {
        let overlay = DocumentOverlay::new();
        let found = overlay.get_symbol_at_position("nonexistent.rs", 1, 0);
        assert!(found.is_none());
    }

    // === find_symbols_by_name tests ===

    #[test]
    fn test_find_symbols_by_name_single_match() {
        let overlay = DocumentOverlay::new();
        overlay.update("test.rs", "fn foo() {}", 1);

        let symbol = create_test_symbol("foo", "test.rs", 1, 3);
        overlay.set_symbols("test.rs", vec![symbol]);

        let found = overlay.find_symbols_by_name("foo");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "foo");
    }

    #[test]
    fn test_find_symbols_by_name_multiple_files() {
        let overlay = DocumentOverlay::new();
        overlay.update("a.rs", "fn foo() {}", 1);
        overlay.update("b.rs", "fn foo() {}", 1);

        overlay.set_symbols("a.rs", vec![create_test_symbol("foo", "a.rs", 1, 3)]);
        overlay.set_symbols("b.rs", vec![create_test_symbol("foo", "b.rs", 1, 3)]);

        let found = overlay.find_symbols_by_name("foo");
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn test_find_symbols_by_name_case_insensitive() {
        let overlay = DocumentOverlay::new();
        overlay.update("test.rs", "fn Foo() {}", 1);

        let symbol = create_test_symbol("Foo", "test.rs", 1, 3);
        overlay.set_symbols("test.rs", vec![symbol]);

        let found = overlay.find_symbols_by_name("foo");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Foo");
    }

    #[test]
    fn test_find_symbols_by_name_no_match() {
        let overlay = DocumentOverlay::new();
        overlay.update("test.rs", "fn bar() {}", 1);

        let symbol = create_test_symbol("bar", "test.rs", 1, 3);
        overlay.set_symbols("test.rs", vec![symbol]);

        let found = overlay.find_symbols_by_name("foo");
        assert!(found.is_empty());
    }

    // === VirtualDocument tests ===

    #[test]
    fn test_virtual_document_update_clears_symbols() {
        let mut doc = VirtualDocument::new("initial".to_string(), 1);
        doc.symbols = vec![create_test_symbol("foo", "test.rs", 1, 3)];
        doc.scopes = vec![Scope {
            id: 1,
            file_path: "test.rs".to_string(),
            parent_id: None,
            kind: ScopeKind::File,
            name: None,
            start_offset: 0,
            end_offset: 100,
            start_line: 1,
            end_line: 10,
        }];

        doc.update("new content".to_string());

        assert!(doc.symbols.is_empty());
        assert!(doc.scopes.is_empty());
        assert_eq!(doc.version, 2);
        assert!(doc.dirty);
    }

    #[test]
    fn test_virtual_document_mark_clean() {
        let mut doc = VirtualDocument::new("content".to_string(), 1);
        assert!(doc.dirty);

        doc.mark_clean();
        assert!(!doc.dirty);
    }

    // === Overlay symbols and scopes management ===

    #[test]
    fn test_overlay_set_and_get_symbols() {
        let overlay = DocumentOverlay::new();
        overlay.update("test.rs", "content", 1);

        let symbols = vec![
            create_test_symbol("foo", "test.rs", 1, 5),
            create_test_symbol("bar", "test.rs", 10, 15),
        ];
        overlay.set_symbols("test.rs", symbols);

        let retrieved = overlay.get_symbols("test.rs");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().len(), 2);
    }

    #[test]
    fn test_overlay_set_symbols_nonexistent_file() {
        let overlay = DocumentOverlay::new();
        // Setting symbols for a file not in overlay should do nothing
        overlay.set_symbols("nonexistent.rs", vec![create_test_symbol("foo", "nonexistent.rs", 1, 3)]);

        let retrieved = overlay.get_symbols("nonexistent.rs");
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_overlay_set_and_get_scopes() {
        let overlay = DocumentOverlay::new();
        overlay.update("test.rs", "content", 1);

        let scopes = vec![Scope {
            id: 1,
            file_path: "test.rs".to_string(),
            parent_id: None,
            kind: ScopeKind::File,
            name: None,
            start_offset: 0,
            end_offset: 100,
            start_line: 1,
            end_line: 10,
        }];
        overlay.set_scopes("test.rs", scopes);

        let retrieved = overlay.get_scopes("test.rs");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().len(), 1);
    }

    #[test]
    fn test_overlay_clear() {
        let overlay = DocumentOverlay::new();
        overlay.update("a.rs", "content a", 1);
        overlay.update("b.rs", "content b", 2);

        overlay.clear();

        assert!(!overlay.contains("a.rs"));
        assert!(!overlay.contains("b.rs"));
        assert!(overlay.all_paths().is_empty());
    }

    #[test]
    fn test_overlay_all_paths() {
        let overlay = DocumentOverlay::new();
        overlay.update("a.rs", "content a", 1);
        overlay.update("b.rs", "content b", 2);

        let paths = overlay.all_paths();
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&"a.rs".to_string()));
        assert!(paths.contains(&"b.rs".to_string()));
    }

    #[test]
    fn test_overlay_get_version() {
        let overlay = DocumentOverlay::new();
        overlay.update("test.rs", "content", 42);

        assert_eq!(overlay.get_version("test.rs"), Some(42));
        assert_eq!(overlay.get_version("nonexistent.rs"), None);
    }
}
