//! Scope Resolver for resolving identifiers within scope chains
//!
//! This module provides scope-aware symbol resolution, walking up the
//! scope chain to find the correct definition of an identifier.

use crate::error::Result;
use crate::index::sqlite::SqliteIndex;
use crate::index::{CodeIndex, Scope, ScopeKind, Symbol};

/// Resolver for looking up symbols within scope chains
pub struct ScopeResolver<'a> {
    index: &'a SqliteIndex,
}

impl<'a> ScopeResolver<'a> {
    /// Creates a new scope resolver
    pub fn new(index: &'a SqliteIndex) -> Self {
        Self { index }
    }

    /// Resolves an identifier starting from a given scope
    ///
    /// Walks up the scope chain looking for a symbol with the given name.
    /// Returns the first matching symbol found.
    pub fn resolve(&self, name: &str, file_path: &str, offset: u32) -> Result<Option<Symbol>> {
        // Get all scopes for the file
        let scopes = self.index.get_file_scopes(file_path)?;

        // Find the scope chain at this offset
        let scope_chain = self.build_scope_chain(&scopes, offset);

        // Walk up the scope chain looking for the symbol
        for scope in scope_chain {
            if let Some(symbol) = self.find_in_scope(name, scope)? {
                return Ok(Some(symbol));
            }
        }

        // If not found in local scopes, search globally
        self.find_global(name)
    }

    /// Resolves a qualified name (e.g., "module::Type::method")
    pub fn resolve_qualified(&self, qualified_name: &str, file_path: &str) -> Result<Option<Symbol>> {
        let parts: Vec<&str> = qualified_name.split("::").collect();

        if parts.is_empty() {
            return Ok(None);
        }

        // Start by finding the first part
        let first = parts[0];

        // Try to find the root symbol
        let mut current_symbols = self.index.find_definition(first)?;

        if current_symbols.is_empty() {
            // Check imports for aliased names
            let imports = self.index.get_file_imports(file_path)?;
            for import in imports {
                if import.imported_symbol.as_deref() == Some(first) {
                    if let Some(ref path) = import.imported_path {
                        // Combine import path with qualified name
                        let full_path = if parts.len() > 1 {
                            format!("{}::{}", path, parts[1..].join("::"))
                        } else {
                            path.clone()
                        };
                        return self.resolve_qualified(&full_path, file_path);
                    }
                }
            }
            return Ok(None);
        }

        // If only one part, return the found symbol
        if parts.len() == 1 {
            return Ok(current_symbols.into_iter().next());
        }

        // Walk the qualified path
        for part in &parts[1..] {
            let mut next_symbols = Vec::new();
            for symbol in &current_symbols {
                // Find members of this symbol
                let members = self.index.get_symbol_members(&symbol.name)?;
                for member in members {
                    if member.name == *part {
                        next_symbols.push(member);
                        break;
                    }
                }
                if !next_symbols.is_empty() {
                    break;
                }
            }
            if next_symbols.is_empty() {
                return Ok(None);
            }
            current_symbols = next_symbols;
        }

        Ok(current_symbols.into_iter().next())
    }

    /// Builds the scope chain from innermost to outermost for a given offset
    fn build_scope_chain<'s>(&self, scopes: &'s [Scope], offset: u32) -> Vec<&'s Scope> {
        let mut chain: Vec<&Scope> = scopes
            .iter()
            .filter(|s| s.start_offset <= offset && s.end_offset >= offset)
            .collect();

        // Sort by scope size (smallest first = innermost)
        chain.sort_by_key(|s| s.end_offset - s.start_offset);
        chain
    }

    /// Finds a symbol within a specific scope
    fn find_in_scope(&self, name: &str, scope: &Scope) -> Result<Option<Symbol>> {
        // Get symbols in the file and filter by scope
        let file_symbols = self.index.get_file_symbols(&scope.file_path)?;

        for symbol in file_symbols {
            if symbol.name == name {
                // Check if symbol is within this scope
                let sym_offset = self.line_to_offset(&scope.file_path, symbol.location.start_line);
                if sym_offset >= scope.start_offset && sym_offset <= scope.end_offset {
                    // For function/class scopes, ensure proper containment
                    if scope.kind == ScopeKind::Function || scope.kind == ScopeKind::Class {
                        if symbol.location.start_line >= scope.start_line
                            && symbol.location.end_line <= scope.end_line
                        {
                            return Ok(Some(symbol));
                        }
                    } else {
                        return Ok(Some(symbol));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Finds a symbol in global scope
    fn find_global(&self, name: &str) -> Result<Option<Symbol>> {
        let symbols = self.index.find_definition(name)?;
        Ok(symbols.into_iter().next())
    }

    /// Converts a line number to an approximate byte offset
    /// This is a simplified approximation - in production you'd use actual line offsets
    fn line_to_offset(&self, _file_path: &str, line: u32) -> u32 {
        // Approximate: assume average 40 chars per line
        // In a real implementation, we'd track actual line offsets
        line.saturating_sub(1) * 40
    }

    /// Gets all visible symbols at a given position
    pub fn visible_symbols(&self, file_path: &str, offset: u32) -> Result<Vec<Symbol>> {
        let mut visible = Vec::new();

        // Get all scopes for the file
        let scopes = self.index.get_file_scopes(file_path)?;
        let scope_chain = self.build_scope_chain(&scopes, offset);

        // Get symbols from each scope in the chain
        let file_symbols = self.index.get_file_symbols(file_path)?;

        for scope in scope_chain {
            for symbol in &file_symbols {
                let sym_offset = self.line_to_offset(file_path, symbol.location.start_line);
                if sym_offset >= scope.start_offset && sym_offset <= scope.end_offset {
                    // Don't add duplicates
                    if !visible.iter().any(|s: &Symbol| s.id == symbol.id) {
                        visible.push(symbol.clone());
                    }
                }
            }
        }

        // Add imported symbols
        let imports = self.index.get_file_imports(file_path)?;
        for import in imports {
            if let Some(ref symbol_name) = import.imported_symbol {
                if let Some(symbol) = self.find_global(symbol_name)? {
                    if !visible.iter().any(|s: &Symbol| s.id == symbol.id) {
                        visible.push(symbol);
                    }
                }
            }
        }

        Ok(visible)
    }
}

/// Helper to compute the FQDN (Fully Qualified Domain Name) for a symbol
pub fn compute_fqdn(symbol: &Symbol, scopes: &[Scope], imports: &[crate::index::FileImport]) -> String {
    let mut parts = Vec::new();

    // Find the scope chain for this symbol
    let offset = symbol.location.start_line.saturating_sub(1) * 40; // Approximate
    let mut scope_chain: Vec<&Scope> = scopes
        .iter()
        .filter(|s| s.start_offset <= offset && s.end_offset >= offset)
        .collect();
    scope_chain.sort_by_key(|s| s.end_offset - s.start_offset);

    // Build path from outermost to innermost
    for scope in scope_chain.iter().rev() {
        if let Some(ref name) = scope.name {
            parts.push(name.clone());
        }
    }

    // Add the symbol name
    parts.push(symbol.name.clone());

    // Check if there's a module/package import prefix
    // This is language-specific and simplified here
    if let Some(file_scope) = scope_chain.last() {
        if file_scope.kind == ScopeKind::File {
            // For Rust, try to extract crate/module path
            for import in imports {
                if import.imported_symbol.as_ref() == Some(&symbol.name) {
                    if let Some(ref path) = import.imported_path {
                        return format!("{}::{}", path, symbol.name);
                    }
                }
            }
        }
    }

    parts.join("::")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{Location, SymbolKind};

    #[test]
    fn test_compute_fqdn_simple() {
        let symbol = Symbol::new(
            "my_function",
            SymbolKind::Function,
            Location::new("test.rs", 10, 0, 20, 1),
            "rust",
        );

        let scopes = vec![
            Scope {
                id: 1,
                file_path: "test.rs".to_string(),
                parent_id: None,
                kind: ScopeKind::File,
                name: None,
                start_offset: 0,
                end_offset: 1000,
                start_line: 1,
                end_line: 50,
            },
            Scope {
                id: 2,
                file_path: "test.rs".to_string(),
                parent_id: Some(1),
                kind: ScopeKind::Module,
                name: Some("mymodule".to_string()),
                start_offset: 100,
                end_offset: 800,
                start_line: 5,
                end_line: 40,
            },
        ];

        let fqdn = compute_fqdn(&symbol, &scopes, &[]);
        assert_eq!(fqdn, "mymodule::my_function");
    }

    #[test]
    fn test_compute_fqdn_nested() {
        let symbol = Symbol::new(
            "method",
            SymbolKind::Method,
            Location::new("test.rs", 15, 0, 20, 1),
            "rust",
        );

        let scopes = vec![
            Scope {
                id: 1,
                file_path: "test.rs".to_string(),
                parent_id: None,
                kind: ScopeKind::File,
                name: None,
                start_offset: 0,
                end_offset: 1000,
                start_line: 1,
                end_line: 50,
            },
            Scope {
                id: 2,
                file_path: "test.rs".to_string(),
                parent_id: Some(1),
                kind: ScopeKind::Module,
                name: Some("mymodule".to_string()),
                start_offset: 100,
                end_offset: 800,
                start_line: 5,
                end_line: 40,
            },
            Scope {
                id: 3,
                file_path: "test.rs".to_string(),
                parent_id: Some(2),
                kind: ScopeKind::Class,
                name: Some("MyStruct".to_string()),
                start_offset: 400,
                end_offset: 700,
                start_line: 10,
                end_line: 35,
            },
        ];

        let fqdn = compute_fqdn(&symbol, &scopes, &[]);
        assert_eq!(fqdn, "mymodule::MyStruct::method");
    }
}
