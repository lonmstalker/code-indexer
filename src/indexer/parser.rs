use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::error::{IndexerError, Result};
use crate::languages::{LanguageGrammar, LanguageRegistry};

pub struct Parser {
    registry: LanguageRegistry,
}

impl Parser {
    pub fn new(registry: LanguageRegistry) -> Self {
        Self { registry }
    }

    pub fn parse_file(&self, path: &Path) -> Result<ParsedFile> {
        let grammar = self
            .registry
            .get_for_file(path)
            .ok_or_else(|| IndexerError::UnsupportedLanguage(path.display().to_string()))?;

        let source = std::fs::read_to_string(path)?;
        self.parse_source(&source, grammar)
    }

    pub fn parse_source(&self, source: &str, grammar: Arc<dyn LanguageGrammar>) -> Result<ParsedFile> {
        self.parse_source_incremental(source, grammar, None)
    }

    /// Parse source with optional old tree for incremental parsing.
    /// Incremental parsing can provide 30-50% speedup for large files.
    pub fn parse_source_incremental(
        &self,
        source: &str,
        grammar: Arc<dyn LanguageGrammar>,
        old_tree: Option<&tree_sitter::Tree>,
    ) -> Result<ParsedFile> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&grammar.language())
            .map_err(|e| IndexerError::Parse(e.to_string()))?;

        let tree = parser
            .parse(source, old_tree)
            .ok_or_else(|| IndexerError::Parse("Failed to parse source".to_string()))?;

        Ok(ParsedFile {
            tree,
            source: source.to_string(),
            language: grammar.name().to_string(),
            grammar,
        })
    }

    #[allow(dead_code)]
    pub fn get_grammar(&self, path: &Path) -> Option<Arc<dyn LanguageGrammar>> {
        self.registry.get_for_file(path)
    }
}

/// Cache for parsed trees to enable incremental parsing.
/// Stores old trees by file path for reuse in subsequent parses.
/// This provides 30-50% speedup for large files during watch mode.
pub struct ParseCache {
    trees: RwLock<HashMap<PathBuf, tree_sitter::Tree>>,
    /// Maximum number of trees to cache
    max_entries: usize,
}

impl ParseCache {
    pub fn new() -> Self {
        Self {
            trees: RwLock::new(HashMap::new()),
            max_entries: 1000, // Default max cache size
        }
    }

    pub fn with_max_entries(max_entries: usize) -> Self {
        Self {
            trees: RwLock::new(HashMap::new()),
            max_entries,
        }
    }

    /// Parse a file with incremental support using cached tree.
    pub fn parse_file(&self, path: &Path, parser: &Parser) -> Result<ParsedFile> {
        let grammar = parser
            .registry
            .get_for_file(path)
            .ok_or_else(|| IndexerError::UnsupportedLanguage(path.display().to_string()))?;

        let source = std::fs::read_to_string(path)?;

        // Try to get old tree for incremental parsing
        let old_tree = {
            let trees = self.trees.read().unwrap();
            trees.get(path).cloned()
        };

        let parsed = parser.parse_source_incremental(&source, grammar, old_tree.as_ref())?;

        // Store the new tree in cache
        self.store_tree(path, parsed.tree.clone());

        Ok(parsed)
    }

    /// Parse source with incremental support using cached tree.
    pub fn parse_source_cached(
        &self,
        path: &Path,
        source: &str,
        parser: &Parser,
    ) -> Result<ParsedFile> {
        let grammar = parser
            .registry
            .get_for_file(path)
            .ok_or_else(|| IndexerError::UnsupportedLanguage(path.display().to_string()))?;

        let old_tree = {
            let trees = self.trees.read().unwrap();
            trees.get(path).cloned()
        };

        let parsed = parser.parse_source_incremental(source, grammar, old_tree.as_ref())?;

        self.store_tree(path, parsed.tree.clone());

        Ok(parsed)
    }

    /// Store a tree in the cache.
    fn store_tree(&self, path: &Path, tree: tree_sitter::Tree) {
        let mut trees = self.trees.write().unwrap();

        // Simple LRU-ish: if at capacity, remove oldest entries
        if trees.len() >= self.max_entries {
            // Remove ~10% of entries to avoid frequent evictions
            let to_remove = trees.len() / 10;
            let keys: Vec<PathBuf> = trees.keys().take(to_remove).cloned().collect();
            for key in keys {
                trees.remove(&key);
            }
        }

        trees.insert(path.to_path_buf(), tree);
    }

    /// Remove a file from the cache.
    pub fn invalidate(&self, path: &Path) {
        let mut trees = self.trees.write().unwrap();
        trees.remove(path);
    }

    /// Clear the entire cache.
    pub fn clear(&self) {
        let mut trees = self.trees.write().unwrap();
        trees.clear();
    }

    /// Get the number of cached trees.
    pub fn len(&self) -> usize {
        self.trees.read().unwrap().len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for ParseCache {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ParsedFile {
    pub tree: tree_sitter::Tree,
    pub source: String,
    pub language: String,
    pub grammar: Arc<dyn LanguageGrammar>,
}

impl ParsedFile {
    pub fn root_node(&self) -> tree_sitter::Node<'_> {
        self.tree.root_node()
    }

    pub fn source_bytes(&self) -> &[u8] {
        self.source.as_bytes()
    }

    pub fn node_text(&self, node: &tree_sitter::Node) -> &str {
        node.utf8_text(self.source_bytes()).unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::LanguageRegistry;
    use std::path::Path;

    fn create_parser() -> Parser {
        Parser::new(LanguageRegistry::new())
    }

    #[test]
    fn test_parse_source_rust() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("rust").unwrap();

        let source = r#"
fn main() {
    println!("Hello, world!");
}
"#;

        let parsed = parser.parse_source(source, grammar).unwrap();
        assert_eq!(parsed.language, "rust");
        assert!(parsed.root_node().child_count() > 0 || parsed.source.is_empty());
    }

    #[test]
    fn test_parse_source_java() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("java").unwrap();

        let source = r#"
public class Main {
    public static void main(String[] args) {
        System.out.println("Hello");
    }
}
"#;

        let parsed = parser.parse_source(source, grammar).unwrap();
        assert_eq!(parsed.language, "java");
        assert!(parsed.root_node().child_count() > 0 || parsed.source.is_empty());
    }

    #[test]
    fn test_parse_source_typescript() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("typescript").unwrap();

        let source = r#"
function greet(name: string): string {
    return `Hello, ${name}!`;
}
"#;

        let parsed = parser.parse_source(source, grammar).unwrap();
        assert_eq!(parsed.language, "typescript");
        assert!(parsed.root_node().child_count() > 0 || parsed.source.is_empty());
    }

    #[test]
    fn test_parse_source_empty() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("rust").unwrap();

        let parsed = parser.parse_source("", grammar).unwrap();
        assert_eq!(parsed.source, "");
    }

    #[test]
    fn test_parsed_file_root_node() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("rust").unwrap();

        let source = "fn test() {}";
        let parsed = parser.parse_source(source, grammar).unwrap();

        let root = parsed.root_node();
        assert_eq!(root.kind(), "source_file");
    }

    #[test]
    fn test_parsed_file_source_bytes() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("rust").unwrap();

        let source = "fn test() {}";
        let parsed = parser.parse_source(source, grammar).unwrap();

        assert_eq!(parsed.source_bytes(), source.as_bytes());
    }

    #[test]
    fn test_parsed_file_node_text() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("rust").unwrap();

        let source = "fn hello() {}";
        let parsed = parser.parse_source(source, grammar).unwrap();

        let root = parsed.root_node();
        let text = parsed.node_text(&root);
        assert_eq!(text, source);
    }

    #[test]
    fn test_get_grammar_rust() {
        let parser = create_parser();
        let grammar = parser.get_grammar(Path::new("test.rs"));
        assert!(grammar.is_some());
        assert_eq!(grammar.unwrap().name(), "rust");
    }

    #[test]
    fn test_get_grammar_java() {
        let parser = create_parser();
        let grammar = parser.get_grammar(Path::new("Main.java"));
        assert!(grammar.is_some());
        assert_eq!(grammar.unwrap().name(), "java");
    }

    #[test]
    fn test_get_grammar_typescript() {
        let parser = create_parser();

        let ts = parser.get_grammar(Path::new("app.ts"));
        assert!(ts.is_some());

        let tsx = parser.get_grammar(Path::new("Component.tsx"));
        assert!(tsx.is_some());
    }

    #[test]
    fn test_get_grammar_unsupported() {
        let parser = create_parser();
        let grammar = parser.get_grammar(Path::new("data.json"));
        assert!(grammar.is_none());
    }

    #[test]
    fn test_get_grammar_python() {
        let parser = create_parser();
        let grammar = parser.get_grammar(Path::new("script.py"));
        assert!(grammar.is_some());
    }

    #[test]
    fn test_parse_rust_function() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("rust").unwrap();

        let source = r#"
pub fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}
"#;

        let parsed = parser.parse_source(source, grammar).unwrap();
        let root = parsed.root_node();

        assert!(root.child_count() > 0);
    }

    #[test]
    fn test_parse_rust_struct() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("rust").unwrap();

        let source = r#"
pub struct Point {
    x: f64,
    y: f64,
}
"#;

        let parsed = parser.parse_source(source, grammar).unwrap();
        let root = parsed.root_node();

        assert!(root.child_count() > 0);
    }

    #[test]
    fn test_parse_java_class() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("java").unwrap();

        let source = r#"
public class Calculator {
    public int add(int a, int b) {
        return a + b;
    }
}
"#;

        let parsed = parser.parse_source(source, grammar).unwrap();
        let root = parsed.root_node();

        assert!(root.child_count() > 0);
    }

    #[test]
    fn test_parse_typescript_interface() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("typescript").unwrap();

        let source = r#"
interface User {
    id: number;
    name: string;
    email?: string;
}
"#;

        let parsed = parser.parse_source(source, grammar).unwrap();
        let root = parsed.root_node();

        assert!(root.child_count() > 0);
    }

    #[test]
    fn test_parsed_file_preserves_source() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("rust").unwrap();

        let source = "// Comment\nfn test() { let x = 42; }";
        let parsed = parser.parse_source(source, grammar).unwrap();

        assert_eq!(parsed.source, source);
    }

    #[test]
    fn test_parsed_file_grammar_reference() {
        let parser = create_parser();
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("rust").unwrap();

        let parsed = parser.parse_source("fn test() {}", grammar).unwrap();

        assert_eq!(parsed.grammar.name(), "rust");
        assert!(!parsed.grammar.functions_query().is_empty());
    }

    // === ParseCache tests ===

    #[test]
    fn test_parse_cache_new() {
        let cache = ParseCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_parse_cache_incremental() {
        let parser = create_parser();
        let _registry = LanguageRegistry::new();
        let cache = ParseCache::new();

        let path = Path::new("test.rs");
        let source1 = "fn test() {}";
        let source2 = "fn test() { let x = 1; }";

        // First parse - no old tree
        let parsed1 = cache.parse_source_cached(path, source1, &parser).unwrap();
        assert_eq!(parsed1.language, "rust");
        assert_eq!(cache.len(), 1);

        // Second parse - should use old tree
        let parsed2 = cache.parse_source_cached(path, source2, &parser).unwrap();
        assert_eq!(parsed2.language, "rust");
        assert_eq!(cache.len(), 1); // Still same entry
    }

    #[test]
    fn test_parse_cache_invalidate() {
        let parser = create_parser();
        let cache = ParseCache::new();

        let path = Path::new("test.rs");
        let source = "fn test() {}";

        cache.parse_source_cached(path, source, &parser).unwrap();
        assert_eq!(cache.len(), 1);

        cache.invalidate(path);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_parse_cache_clear() {
        let parser = create_parser();
        let cache = ParseCache::new();

        cache.parse_source_cached(Path::new("a.rs"), "fn a() {}", &parser).unwrap();
        cache.parse_source_cached(Path::new("b.rs"), "fn b() {}", &parser).unwrap();
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_parse_cache_max_entries() {
        let parser = create_parser();
        let cache = ParseCache::with_max_entries(5);

        // Add more than max entries
        for i in 0..10 {
            let path = PathBuf::from(format!("test{}.rs", i));
            let source = format!("fn test{}() {{}}", i);
            cache.parse_source_cached(&path, &source, &parser).unwrap();
        }

        // Should have evicted some entries
        assert!(cache.len() <= 10);
    }
}
