//! Scope Builder for constructing scope trees from AST
//!
//! This module builds a hierarchical scope tree from parsed source code,
//! enabling scope-aware symbol resolution.

use tree_sitter::Node;

use crate::index::{Scope, ScopeKind};
use crate::indexer::parser::ParsedFile;

/// Builder for constructing scope trees from AST
pub struct ScopeBuilder {
    next_id: i64,
}

impl Default for ScopeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopeBuilder {
    /// Creates a new scope builder
    pub fn new() -> Self {
        Self { next_id: 1 }
    }

    /// Builds a scope tree from a parsed file
    pub fn build(&mut self, parsed: &ParsedFile, file_path: &str) -> Vec<Scope> {
        let mut scopes = Vec::new();
        let root = parsed.tree.root_node();

        // Create file-level scope
        let file_scope = Scope {
            id: self.next_id(),
            file_path: file_path.to_string(),
            parent_id: None,
            kind: ScopeKind::File,
            name: None,
            start_offset: root.start_byte() as u32,
            end_offset: root.end_byte() as u32,
            start_line: root.start_position().row as u32 + 1,
            end_line: root.end_position().row as u32 + 1,
        };
        let file_scope_id = file_scope.id;
        scopes.push(file_scope);

        // Recursively build scopes
        self.build_scopes_recursive(
            &root,
            &parsed.source,
            file_path,
            &parsed.language,
            Some(file_scope_id),
            &mut scopes,
        );

        scopes
    }

    /// Recursively builds scopes from AST nodes
    fn build_scopes_recursive(
        &mut self,
        node: &Node,
        source: &str,
        file_path: &str,
        language: &str,
        parent_id: Option<i64>,
        scopes: &mut Vec<Scope>,
    ) {
        // Check if this node creates a scope
        if let Some(scope) = self.node_to_scope(node, source, file_path, language, parent_id) {
            let scope_id = scope.id;
            scopes.push(scope);

            // Process children with this scope as parent
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.build_scopes_recursive(&child, source, file_path, language, Some(scope_id), scopes);
            }
        } else {
            // Process children with same parent
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.build_scopes_recursive(&child, source, file_path, language, parent_id, scopes);
            }
        }
    }

    /// Converts an AST node to a scope if applicable
    fn node_to_scope(
        &mut self,
        node: &Node,
        source: &str,
        file_path: &str,
        language: &str,
        parent_id: Option<i64>,
    ) -> Option<Scope> {
        let kind = node.kind();

        let (scope_kind, name) = match language {
            "rust" => self.rust_node_to_scope(node, source, kind)?,
            "java" | "kotlin" => self.java_node_to_scope(node, source, kind)?,
            "typescript" | "javascript" | "tsx" => self.ts_node_to_scope(node, source, kind)?,
            "python" => self.python_node_to_scope(node, source, kind)?,
            "go" => self.go_node_to_scope(node, source, kind)?,
            "csharp" => self.csharp_node_to_scope(node, source, kind)?,
            "cpp" | "c" => self.cpp_node_to_scope(node, source, kind)?,
            _ => return None,
        };

        Some(Scope {
            id: self.next_id(),
            file_path: file_path.to_string(),
            parent_id,
            kind: scope_kind,
            name,
            start_offset: node.start_byte() as u32,
            end_offset: node.end_byte() as u32,
            start_line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
        })
    }

    /// Rust-specific scope detection
    fn rust_node_to_scope(&self, node: &Node, source: &str, kind: &str) -> Option<(ScopeKind, Option<String>)> {
        match kind {
            "mod_item" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Module, name))
            }
            "function_item" | "function_signature_item" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Function, name))
            }
            "impl_item" | "struct_item" | "enum_item" | "trait_item" => {
                let name = self.get_child_by_field(node, "name", source)
                    .or_else(|| self.get_child_by_field(node, "type", source));
                Some((ScopeKind::Class, name))
            }
            "closure_expression" => Some((ScopeKind::Closure, None)),
            "block" if self.is_standalone_block(node) => Some((ScopeKind::Block, None)),
            _ => None,
        }
    }

    /// Java/Kotlin-specific scope detection
    fn java_node_to_scope(&self, node: &Node, source: &str, kind: &str) -> Option<(ScopeKind, Option<String>)> {
        match kind {
            "package_declaration" => None, // Package is not a scope per se
            "class_declaration" | "interface_declaration" | "enum_declaration" | "annotation_type_declaration" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Class, name))
            }
            "method_declaration" | "constructor_declaration" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Function, name))
            }
            "lambda_expression" => Some((ScopeKind::Closure, None)),
            "block" if self.is_standalone_block(node) => Some((ScopeKind::Block, None)),
            _ => None,
        }
    }

    /// TypeScript/JavaScript-specific scope detection
    fn ts_node_to_scope(&self, node: &Node, source: &str, kind: &str) -> Option<(ScopeKind, Option<String>)> {
        match kind {
            "class_declaration" | "abstract_class_declaration" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Class, name))
            }
            "interface_declaration" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Class, name))
            }
            "function_declaration" | "generator_function_declaration" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Function, name))
            }
            "method_definition" | "function" | "generator_function" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Function, name))
            }
            "arrow_function" => Some((ScopeKind::Closure, None)),
            "module" | "internal_module" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Module, name))
            }
            _ => None,
        }
    }

    /// Python-specific scope detection
    fn python_node_to_scope(&self, node: &Node, source: &str, kind: &str) -> Option<(ScopeKind, Option<String>)> {
        match kind {
            "module" => Some((ScopeKind::Module, None)),
            "class_definition" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Class, name))
            }
            "function_definition" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Function, name))
            }
            "lambda" => Some((ScopeKind::Closure, None)),
            _ => None,
        }
    }

    /// Go-specific scope detection
    fn go_node_to_scope(&self, node: &Node, source: &str, kind: &str) -> Option<(ScopeKind, Option<String>)> {
        match kind {
            "package_clause" => None, // Package is file-level in Go
            "type_declaration" => {
                // Go type declarations (struct, interface)
                let name = self.get_first_identifier(node, source);
                Some((ScopeKind::Class, name))
            }
            "function_declaration" | "method_declaration" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Function, name))
            }
            "func_literal" => Some((ScopeKind::Closure, None)),
            _ => None,
        }
    }

    /// C#-specific scope detection
    fn csharp_node_to_scope(&self, node: &Node, source: &str, kind: &str) -> Option<(ScopeKind, Option<String>)> {
        match kind {
            "namespace_declaration" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Module, name))
            }
            "class_declaration" | "struct_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Class, name))
            }
            "method_declaration" | "constructor_declaration" | "local_function_statement" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Function, name))
            }
            "lambda_expression" | "anonymous_method_expression" => Some((ScopeKind::Closure, None)),
            _ => None,
        }
    }

    /// C/C++-specific scope detection
    fn cpp_node_to_scope(&self, node: &Node, source: &str, kind: &str) -> Option<(ScopeKind, Option<String>)> {
        match kind {
            "namespace_definition" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Module, name))
            }
            "class_specifier" | "struct_specifier" | "enum_specifier" => {
                let name = self.get_child_by_field(node, "name", source);
                Some((ScopeKind::Class, name))
            }
            "function_definition" => {
                let name = self.get_child_by_field(node, "declarator", source)
                    .or_else(|| self.get_first_identifier(node, source));
                Some((ScopeKind::Function, name))
            }
            "lambda_expression" => Some((ScopeKind::Closure, None)),
            _ => None,
        }
    }

    /// Gets a child node's text by field name
    fn get_child_by_field(&self, node: &Node, field: &str, source: &str) -> Option<String> {
        node.child_by_field_name(field)
            .map(|n| source[n.byte_range()].to_string())
    }

    /// Gets the first identifier child of a node
    fn get_first_identifier(&self, node: &Node, source: &str) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" || child.kind() == "type_identifier" {
                return Some(source[child.byte_range()].to_string());
            }
        }
        None
    }

    /// Checks if a block is standalone (not part of a function/if/etc.)
    fn is_standalone_block(&self, node: &Node) -> bool {
        if let Some(parent) = node.parent() {
            let parent_kind = parent.kind();
            // These are control flow/function blocks, not standalone
            !matches!(
                parent_kind,
                "function_item"
                    | "function_declaration"
                    | "method_declaration"
                    | "method_definition"
                    | "if_expression"
                    | "if_statement"
                    | "while_expression"
                    | "while_statement"
                    | "for_expression"
                    | "for_statement"
                    | "loop_expression"
                    | "match_arm"
                    | "try_statement"
                    | "catch_clause"
                    | "finally_clause"
            )
        } else {
            false
        }
    }

    /// Generates the next scope ID
    fn next_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Finds the innermost scope containing a given offset
pub fn scope_at_offset(scopes: &[Scope], offset: u32) -> Option<&Scope> {
    scopes
        .iter()
        .filter(|s| s.start_offset <= offset && s.end_offset >= offset)
        .min_by_key(|s| s.end_offset - s.start_offset)
}

/// Finds the scope chain from innermost to outermost for a given offset
pub fn scope_chain(scopes: &[Scope], offset: u32) -> Vec<&Scope> {
    let mut chain: Vec<&Scope> = scopes
        .iter()
        .filter(|s| s.start_offset <= offset && s.end_offset >= offset)
        .collect();

    // Sort by scope size (smallest first = innermost)
    chain.sort_by_key(|s| s.end_offset - s.start_offset);
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_at_offset() {
        let scopes = vec![
            Scope {
                id: 1,
                file_path: "test.rs".to_string(),
                parent_id: None,
                kind: ScopeKind::File,
                name: None,
                start_offset: 0,
                end_offset: 100,
                start_line: 1,
                end_line: 10,
            },
            Scope {
                id: 2,
                file_path: "test.rs".to_string(),
                parent_id: Some(1),
                kind: ScopeKind::Function,
                name: Some("main".to_string()),
                start_offset: 10,
                end_offset: 50,
                start_line: 2,
                end_line: 5,
            },
        ];

        // Inside function
        let scope = scope_at_offset(&scopes, 20).unwrap();
        assert_eq!(scope.id, 2);
        assert_eq!(scope.kind, ScopeKind::Function);

        // Outside function but inside file
        let scope = scope_at_offset(&scopes, 60).unwrap();
        assert_eq!(scope.id, 1);
        assert_eq!(scope.kind, ScopeKind::File);
    }

    #[test]
    fn test_scope_chain() {
        let scopes = vec![
            Scope {
                id: 1,
                file_path: "test.rs".to_string(),
                parent_id: None,
                kind: ScopeKind::File,
                name: None,
                start_offset: 0,
                end_offset: 100,
                start_line: 1,
                end_line: 10,
            },
            Scope {
                id: 2,
                file_path: "test.rs".to_string(),
                parent_id: Some(1),
                kind: ScopeKind::Function,
                name: Some("main".to_string()),
                start_offset: 10,
                end_offset: 50,
                start_line: 2,
                end_line: 5,
            },
            Scope {
                id: 3,
                file_path: "test.rs".to_string(),
                parent_id: Some(2),
                kind: ScopeKind::Block,
                name: None,
                start_offset: 20,
                end_offset: 40,
                start_line: 3,
                end_line: 4,
            },
        ];

        let chain = scope_chain(&scopes, 30);
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0].id, 3); // Block (innermost)
        assert_eq!(chain[1].id, 2); // Function
        assert_eq!(chain[2].id, 1); // File (outermost)
    }
}
