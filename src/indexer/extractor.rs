use std::path::Path;

use tree_sitter::StreamingIterator;

use crate::error::{IndexerError, Result};
use crate::index::{FileImport, ImportType, Location, ReferenceKind, Symbol, SymbolKind, SymbolReference, Visibility};
use crate::indexer::parser::ParsedFile;

/// Result of extraction containing symbols, references, and imports
#[derive(Debug, Default)]
pub struct ExtractionResult {
    pub symbols: Vec<Symbol>,
    pub references: Vec<SymbolReference>,
    pub imports: Vec<FileImport>,
}

pub struct SymbolExtractor;

impl SymbolExtractor {
    pub fn new() -> Self {
        Self
    }

    pub fn extract(&self, parsed: &ParsedFile, file_path: &Path) -> Result<Vec<Symbol>> {
        let result = self.extract_all(parsed, file_path)?;
        Ok(result.symbols)
    }

    /// Extract symbols, references, and imports from a parsed file
    pub fn extract_all(&self, parsed: &ParsedFile, file_path: &Path) -> Result<ExtractionResult> {
        let mut result = ExtractionResult::default();
        let file_path_str = file_path.to_string_lossy().to_string();

        self.extract_functions(parsed, &file_path_str, &mut result.symbols)?;
        self.extract_types(parsed, &file_path_str, &mut result.symbols)?;
        self.extract_references(parsed, &file_path_str, &mut result.references)?;
        self.extract_imports(parsed, &file_path_str, &mut result.imports)?;

        Ok(result)
    }

    fn extract_functions(
        &self,
        parsed: &ParsedFile,
        file_path: &str,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        let query_str = parsed.grammar.functions_query();
        if query_str.trim().is_empty() {
            return Ok(());
        }

        let query = tree_sitter::Query::new(&parsed.grammar.language(), query_str)
            .map_err(|e| IndexerError::Parse(format!("Invalid functions query: {}", e)))?;

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, parsed.root_node(), parsed.source_bytes());

        while let Some(m) = matches.next() {
            let mut name: Option<&str> = None;
            let mut kind = SymbolKind::Function;
            let mut node: Option<tree_sitter::Node> = None;
            let mut signature_parts: Vec<&str> = Vec::new();

            for capture in m.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = parsed.node_text(&capture.node);

                match capture_name {
                    "name" | "method_name" => {
                        name = Some(text);
                        if capture_name == "method_name" {
                            kind = SymbolKind::Method;
                        }
                    }
                    "function" | "method" | "constructor" | "named_arrow" => {
                        node = Some(capture.node);
                        if capture_name == "constructor" {
                            kind = SymbolKind::Method;
                        }
                    }
                    "params" | "method_params" => {
                        signature_parts.push(text);
                    }
                    "return_type" | "method_return_type" => {
                        signature_parts.push(text);
                    }
                    _ => {}
                }
            }

            if let (Some(name), Some(node)) = (name, node) {
                let location = Location::new(
                    file_path,
                    node.start_position().row as u32 + 1,
                    node.start_position().column as u32,
                    node.end_position().row as u32 + 1,
                    node.end_position().column as u32,
                );

                let mut symbol = Symbol::new(name, kind, location, &parsed.language);

                let visibility = self.extract_visibility(parsed, &node);
                if let Some(v) = visibility {
                    symbol = symbol.with_visibility(v);
                }

                let doc = self.extract_doc_comment(parsed, &node);
                if let Some(d) = doc {
                    symbol = symbol.with_doc_comment(d);
                }

                if !signature_parts.is_empty() {
                    symbol =
                        symbol.with_signature(format!("{}{}", name, signature_parts.join(" -> ")));
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    fn extract_types(
        &self,
        parsed: &ParsedFile,
        file_path: &str,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        let query_str = parsed.grammar.types_query();
        if query_str.trim().is_empty() {
            return Ok(());
        }

        let query = tree_sitter::Query::new(&parsed.grammar.language(), query_str)
            .map_err(|e| IndexerError::Parse(format!("Invalid types query: {}", e)))?;

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, parsed.root_node(), parsed.source_bytes());

        while let Some(m) = matches.next() {
            let mut name: Option<&str> = None;
            let mut kind = SymbolKind::Struct;
            let mut node: Option<tree_sitter::Node> = None;

            for capture in m.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = parsed.node_text(&capture.node);

                match capture_name {
                    "name" => {
                        name = Some(text);
                    }
                    "struct" => {
                        node = Some(capture.node);
                        kind = SymbolKind::Struct;
                    }
                    "class" | "record" => {
                        node = Some(capture.node);
                        kind = SymbolKind::Class;
                    }
                    "interface" => {
                        node = Some(capture.node);
                        kind = SymbolKind::Interface;
                    }
                    "trait" => {
                        node = Some(capture.node);
                        kind = SymbolKind::Trait;
                    }
                    "enum" => {
                        node = Some(capture.node);
                        kind = SymbolKind::Enum;
                    }
                    "type_alias" => {
                        node = Some(capture.node);
                        kind = SymbolKind::TypeAlias;
                    }
                    _ => {}
                }
            }

            if let (Some(name), Some(node)) = (name, node) {
                let location = Location::new(
                    file_path,
                    node.start_position().row as u32 + 1,
                    node.start_position().column as u32,
                    node.end_position().row as u32 + 1,
                    node.end_position().column as u32,
                );

                let mut symbol = Symbol::new(name, kind, location, &parsed.language);

                let visibility = self.extract_visibility(parsed, &node);
                if let Some(v) = visibility {
                    symbol = symbol.with_visibility(v);
                }

                let doc = self.extract_doc_comment(parsed, &node);
                if let Some(d) = doc {
                    symbol = symbol.with_doc_comment(d);
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    fn extract_visibility(
        &self,
        parsed: &ParsedFile,
        node: &tree_sitter::Node,
    ) -> Option<Visibility> {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            let kind = child.kind();
            let text = parsed.node_text(&child);

            match kind {
                "visibility_modifier" => {
                    return Visibility::from_str(text);
                }
                "modifiers" => {
                    for modifier in child.children(&mut child.walk()) {
                        let modifier_text = parsed.node_text(&modifier);
                        if let Some(v) = Visibility::from_str(modifier_text) {
                            return Some(v);
                        }
                    }
                }
                _ => {
                    if text == "public" || text == "private" || text == "protected" {
                        return Visibility::from_str(text);
                    }
                }
            }
        }

        None
    }

    fn extract_doc_comment(
        &self,
        parsed: &ParsedFile,
        node: &tree_sitter::Node,
    ) -> Option<String> {
        if let Some(prev) = node.prev_sibling() {
            let kind = prev.kind();
            if kind.contains("comment") || kind == "line_comment" || kind == "block_comment" {
                let text = parsed.node_text(&prev).trim();
                if text.starts_with("///") || text.starts_with("/**") || text.starts_with("//!") {
                    return Some(text.to_string());
                }
            }
        }

        None
    }

    fn extract_references(
        &self,
        parsed: &ParsedFile,
        file_path: &str,
        references: &mut Vec<SymbolReference>,
    ) -> Result<()> {
        let query_str = parsed.grammar.references_query();
        if query_str.trim().is_empty() {
            return Ok(());
        }

        let query = match tree_sitter::Query::new(&parsed.grammar.language(), query_str) {
            Ok(q) => q,
            Err(e) => {
                // Log error but don't fail - references are optional
                tracing::warn!("Invalid references query for {}: {}", parsed.language, e);
                return Ok(());
            }
        };

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, parsed.root_node(), parsed.source_bytes());

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = parsed.node_text(&capture.node);
                let node = capture.node;

                let reference_kind = match capture_name {
                    "call_name" | "method_call_name" | "scoped_call_name" | "macro_name" => {
                        Some(ReferenceKind::Call)
                    }
                    "constructor_call_name" => Some(ReferenceKind::Call),
                    "type_use" => Some(ReferenceKind::TypeUse),
                    "impl_trait" | "extends_type" | "implements_type" => {
                        Some(ReferenceKind::Extend)
                    }
                    "field_access" | "field_access_name" | "property_access" | "static_access_name" => {
                        Some(ReferenceKind::FieldAccess)
                    }
                    _ => None,
                };

                if let Some(kind) = reference_kind {
                    // Skip common built-in types and keywords
                    if !Self::is_builtin_type(text, &parsed.language) {
                        references.push(SymbolReference::new(
                            text,
                            file_path,
                            node.start_position().row as u32 + 1,
                            node.start_position().column as u32,
                            kind,
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    fn extract_imports(
        &self,
        parsed: &ParsedFile,
        file_path: &str,
        imports: &mut Vec<FileImport>,
    ) -> Result<()> {
        let query_str = parsed.grammar.imports_query();
        if query_str.trim().is_empty() {
            return Ok(());
        }

        let query = match tree_sitter::Query::new(&parsed.grammar.language(), query_str) {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!("Invalid imports query for {}: {}", parsed.language, e);
                return Ok(());
            }
        };

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, parsed.root_node(), parsed.source_bytes());

        while let Some(m) = matches.next() {
            let mut import_path: Option<&str> = None;
            let mut is_wildcard = false;

            for capture in m.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = parsed.node_text(&capture.node);

                match capture_name {
                    "import_path" | "source" | "crate_name" => {
                        import_path = Some(text);
                        if text.ends_with('*') || text.contains("::*") {
                            is_wildcard = true;
                        }
                    }
                    _ => {}
                }
            }

            if let Some(path) = import_path {
                let import_type = if is_wildcard {
                    ImportType::Wildcard
                } else if path.contains("::") || path.contains('.') || path.contains('/') {
                    ImportType::Module
                } else {
                    ImportType::Symbol
                };

                // Extract the final symbol name if it's a specific import
                let imported_symbol = if import_type == ImportType::Symbol {
                    Some(path.to_string())
                } else {
                    path.rsplit(|c| c == ':' || c == '.' || c == '/')
                        .next()
                        .filter(|s| !s.is_empty() && *s != "*")
                        .map(|s| s.to_string())
                };

                imports.push(FileImport {
                    file_path: file_path.to_string(),
                    imported_path: Some(path.trim_matches('"').to_string()),
                    imported_symbol,
                    import_type,
                });
            }
        }

        Ok(())
    }

    /// Check if a type name is a built-in/primitive type
    fn is_builtin_type(name: &str, language: &str) -> bool {
        match language {
            "rust" => matches!(
                name,
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
                    | "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
                    | "f32" | "f64" | "bool" | "char" | "str" | "String"
                    | "Self" | "self" | "Vec" | "Option" | "Result" | "Box"
                    | "Rc" | "Arc" | "Cell" | "RefCell" | "Ok" | "Err" | "Some" | "None"
            ),
            "java" | "kotlin" => matches!(
                name,
                "int" | "long" | "short" | "byte" | "float" | "double" | "boolean" | "char"
                    | "void" | "Int" | "Long" | "Short" | "Byte" | "Float" | "Double"
                    | "Boolean" | "Char" | "String" | "Object" | "Unit" | "Any" | "Nothing"
            ),
            "typescript" => matches!(
                name,
                "string" | "number" | "boolean" | "void" | "null" | "undefined"
                    | "any" | "unknown" | "never" | "object" | "symbol" | "bigint"
                    | "String" | "Number" | "Boolean" | "Array" | "Object" | "Promise"
            ),
            _ => false,
        }
    }
}

impl Default for SymbolExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::parser::Parser;
    use crate::languages::LanguageRegistry;
    use std::path::PathBuf;

    fn parse_and_extract(source: &str, language: &str, filename: &str) -> Vec<Symbol> {
        let registry = LanguageRegistry::new();
        let parser = Parser::new(registry);
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name(language).unwrap();
        let parsed = parser.parse_source(source, grammar).unwrap();
        let extractor = SymbolExtractor::new();
        extractor.extract(&parsed, &PathBuf::from(filename)).unwrap()
    }

    // Rust tests
    #[test]
    fn test_extract_rust_function() {
        let source = r#"
fn hello_world() {
    println!("Hello");
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        assert!(!symbols.is_empty());
        let func = symbols.iter().find(|s| s.name == "hello_world");
        assert!(func.is_some());
        assert_eq!(func.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_rust_public_function() {
        let source = r#"
pub fn public_func() -> i32 {
    42
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let func = symbols.iter().find(|s| s.name == "public_func").unwrap();
        assert_eq!(func.visibility, Some(Visibility::Public));
    }

    #[test]
    fn test_extract_rust_struct() {
        let source = r#"
pub struct Point {
    x: f64,
    y: f64,
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let struct_sym = symbols.iter().find(|s| s.name == "Point");
        assert!(struct_sym.is_some());
        assert_eq!(struct_sym.unwrap().kind, SymbolKind::Struct);
    }

    #[test]
    fn test_extract_rust_enum() {
        let source = r#"
pub enum Color {
    Red,
    Green,
    Blue,
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let enum_sym = symbols.iter().find(|s| s.name == "Color");
        assert!(enum_sym.is_some());
        assert_eq!(enum_sym.unwrap().kind, SymbolKind::Enum);
    }

    #[test]
    fn test_extract_rust_trait() {
        let source = r#"
pub trait Drawable {
    fn draw(&self);
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let trait_sym = symbols.iter().find(|s| s.name == "Drawable");
        assert!(trait_sym.is_some());
        assert_eq!(trait_sym.unwrap().kind, SymbolKind::Trait);
    }

    #[test]
    fn test_extract_rust_impl_method() {
        let source = r#"
struct Calculator;

impl Calculator {
    pub fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let method = symbols.iter().find(|s| s.name == "add");
        assert!(method.is_some());
        assert_eq!(method.unwrap().kind, SymbolKind::Method);
    }

    #[test]
    fn test_extract_rust_type_alias() {
        let source = r#"
pub type Result<T> = std::result::Result<T, Error>;
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let alias = symbols.iter().find(|s| s.name == "Result");
        assert!(alias.is_some());
        assert_eq!(alias.unwrap().kind, SymbolKind::TypeAlias);
    }

    #[test]
    fn test_extract_rust_multiple_functions() {
        let source = r#"
fn func_a() {}
fn func_b() {}
fn func_c() {}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let funcs: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Function).collect();
        assert_eq!(funcs.len(), 3);
    }

    #[test]
    fn test_extract_rust_location() {
        let source = r#"fn test() {}"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let func = symbols.iter().find(|s| s.name == "test").unwrap();
        assert_eq!(func.location.file_path, "test.rs");
        assert!(func.location.start_line >= 1);
    }

    // Java tests
    #[test]
    fn test_extract_java_class() {
        let source = r#"
public class Calculator {
}
"#;
        let symbols = parse_and_extract(source, "java", "Calculator.java");

        let class = symbols.iter().find(|s| s.name == "Calculator");
        assert!(class.is_some());
        assert_eq!(class.unwrap().kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_java_method() {
        let source = r#"
public class Math {
    public int add(int a, int b) {
        return a + b;
    }
}
"#;
        let symbols = parse_and_extract(source, "java", "Math.java");

        let method = symbols.iter().find(|s| s.name == "add");
        assert!(method.is_some());
        // Java methods are extracted via @method capture which maps to Method kind
        let kind = method.unwrap().kind.clone();
        assert!(kind == SymbolKind::Method || kind == SymbolKind::Function);
    }

    #[test]
    fn test_extract_java_interface() {
        let source = r#"
public interface Printable {
    void print();
}
"#;
        let symbols = parse_and_extract(source, "java", "Printable.java");

        let iface = symbols.iter().find(|s| s.name == "Printable");
        assert!(iface.is_some());
        assert_eq!(iface.unwrap().kind, SymbolKind::Interface);
    }

    #[test]
    fn test_extract_java_enum() {
        let source = r#"
public enum Status {
    ACTIVE,
    INACTIVE
}
"#;
        let symbols = parse_and_extract(source, "java", "Status.java");

        let enum_sym = symbols.iter().find(|s| s.name == "Status");
        assert!(enum_sym.is_some());
        assert_eq!(enum_sym.unwrap().kind, SymbolKind::Enum);
    }

    #[test]
    fn test_extract_java_record() {
        let source = r#"
public record Point(int x, int y) {}
"#;
        let symbols = parse_and_extract(source, "java", "Point.java");

        let record = symbols.iter().find(|s| s.name == "Point");
        assert!(record.is_some());
        assert_eq!(record.unwrap().kind, SymbolKind::Class);
    }

    // TypeScript tests
    #[test]
    fn test_extract_typescript_function() {
        let source = r#"
function greet(name: string): string {
    return `Hello, ${name}!`;
}
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        let func = symbols.iter().find(|s| s.name == "greet");
        assert!(func.is_some());
        assert_eq!(func.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_typescript_interface() {
        let source = r#"
interface User {
    id: number;
    name: string;
}
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        let iface = symbols.iter().find(|s| s.name == "User");
        assert!(iface.is_some());
        assert_eq!(iface.unwrap().kind, SymbolKind::Interface);
    }

    #[test]
    fn test_extract_typescript_class() {
        let source = r#"
class Calculator {
    add(a: number, b: number): number {
        return a + b;
    }
}
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        let class = symbols.iter().find(|s| s.name == "Calculator");
        assert!(class.is_some());
        assert_eq!(class.unwrap().kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_typescript_type_alias() {
        let source = r#"
type StringOrNumber = string | number;
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        let alias = symbols.iter().find(|s| s.name == "StringOrNumber");
        assert!(alias.is_some());
        assert_eq!(alias.unwrap().kind, SymbolKind::TypeAlias);
    }

    #[test]
    fn test_extract_typescript_enum() {
        let source = r#"
enum Direction {
    Up,
    Down,
    Left,
    Right
}
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        let enum_sym = symbols.iter().find(|s| s.name == "Direction");
        assert!(enum_sym.is_some());
        assert_eq!(enum_sym.unwrap().kind, SymbolKind::Enum);
    }

    #[test]
    fn test_extract_typescript_arrow_function() {
        let source = r#"
const add = (a: number, b: number): number => a + b;
"#;
        let symbols = parse_and_extract(source, "typescript", "test.ts");

        // Arrow functions should be extracted as functions
        let func = symbols.iter().find(|s| s.name == "add");
        assert!(func.is_some(), "Arrow function 'add' should be extracted");
        assert_eq!(func.unwrap().kind, SymbolKind::Function);
    }

    // General tests
    #[test]
    fn test_extract_empty_source() {
        let symbols = parse_and_extract("", "rust", "test.rs");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_language_field() {
        let symbols = parse_and_extract("fn test() {}", "rust", "test.rs");
        assert!(!symbols.is_empty());
        assert_eq!(symbols[0].language, "rust");
    }

    #[test]
    fn test_extractor_default() {
        let extractor = SymbolExtractor::default();
        let registry = LanguageRegistry::new();
        let parser = Parser::new(registry);
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("rust").unwrap();
        let parsed = parser.parse_source("fn test() {}", grammar).unwrap();
        let symbols = extractor.extract(&parsed, &PathBuf::from("test.rs")).unwrap();
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_extract_symbols_have_unique_ids() {
        let source = r#"
fn func1() {}
fn func2() {}
"#;
        let symbols = parse_and_extract(source, "rust", "test.rs");

        let ids: Vec<_> = symbols.iter().map(|s| &s.id).collect();
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique_ids.len());
    }
}
