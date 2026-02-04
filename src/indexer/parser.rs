use std::path::Path;
use std::sync::Arc;

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
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&grammar.language())
            .map_err(|e| IndexerError::Parse(e.to_string()))?;

        let tree = parser
            .parse(source, None)
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
}
