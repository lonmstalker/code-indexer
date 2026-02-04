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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
