//! Import Resolution for resolving import paths to symbols
//!
//! This module provides language-specific import resolution to map
//! import statements to their corresponding symbol definitions.

use std::path::Path;

use crate::error::Result;
use crate::index::{CodeIndex, FileImport, ImportType, Symbol};

/// Trait for language-specific import resolution
pub trait ImportResolver: Send + Sync {
    /// Resolves an import to its target symbol(s)
    fn resolve(&self, import: &FileImport, index: &dyn CodeIndex) -> Result<Vec<Symbol>>;

    /// Computes the FQDN for a symbol based on the file's module structure
    fn compute_fqdn(&self, symbol: &Symbol, file_path: &str) -> String;

    /// Returns the language this resolver handles
    fn language(&self) -> &'static str;
}

/// Rust import resolver
///
/// Handles Rust's `use` statements including:
/// - `use crate::module::Type;`
/// - `use super::sibling::func;`
/// - `use self::submodule::Item;`
/// - `use std::collections::HashMap;`
pub struct RustImportResolver;

impl ImportResolver for RustImportResolver {
    fn resolve(&self, import: &FileImport, index: &dyn CodeIndex) -> Result<Vec<Symbol>> {
        let path = match &import.imported_path {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        // Handle special prefixes
        let normalized_path = self.normalize_rust_path(path, &import.file_path);

        // For wildcard imports, find all symbols in the module
        if import.import_type == ImportType::Wildcard {
            return self.resolve_wildcard(&normalized_path, index);
        }

        // For symbol imports, find the specific symbol
        if let Some(ref symbol_name) = import.imported_symbol {
            return Ok(index.find_definition(symbol_name)?);
        }

        // For module imports, find the module
        let parts: Vec<&str> = normalized_path.split("::").collect();
        if let Some(last) = parts.last() {
            return Ok(index.find_definition(last)?);
        }

        Ok(Vec::new())
    }

    fn compute_fqdn(&self, symbol: &Symbol, file_path: &str) -> String {
        // Extract module path from file path
        let path = Path::new(file_path);

        let mut parts = Vec::new();

        // Try to determine crate name (assuming standard layout)
        if let Some(parent) = path.parent() {
            let path_str = parent.to_string_lossy();
            if path_str.contains("src") {
                // Simple heuristic: use path components after "src"
                let after_src: Vec<&str> = path_str.split("src").collect();
                if after_src.len() > 1 {
                    for component in after_src[1].split(['/', '\\']) {
                        if !component.is_empty() {
                            parts.push(component.to_string());
                        }
                    }
                }
            }
        }

        // Add file name (without extension) if not mod.rs or lib.rs
        if let Some(stem) = path.file_stem() {
            let name = stem.to_string_lossy();
            if name != "mod" && name != "lib" && name != "main" {
                parts.push(name.to_string());
            }
        }

        // Add parent name if exists
        if let Some(ref parent) = symbol.parent {
            parts.push(parent.clone());
        }

        parts.push(symbol.name.clone());
        parts.join("::")
    }

    fn language(&self) -> &'static str {
        "rust"
    }
}

impl RustImportResolver {
    /// Normalizes a Rust import path by handling special prefixes
    fn normalize_rust_path(&self, path: &str, file_path: &str) -> String {
        if path.starts_with("crate::") {
            // Replace crate:: with actual crate name (simplified)
            return path.to_string();
        }

        if path.starts_with("self::") {
            // Current module - strip self:: and use current file's module path
            let module_path = self.file_to_module_path(file_path);
            let rest = &path[6..];
            if module_path.is_empty() {
                return rest.to_string();
            }
            return format!("{}::{}", module_path, rest);
        }

        if path.starts_with("super::") {
            // Parent module
            let module_path = self.file_to_module_path(file_path);
            let parts: Vec<&str> = module_path.split("::").collect();
            let rest = &path[7..];
            if parts.len() > 1 {
                let parent = parts[..parts.len() - 1].join("::");
                return format!("{}::{}", parent, rest);
            }
            return rest.to_string();
        }

        path.to_string()
    }

    /// Converts a file path to a module path
    fn file_to_module_path(&self, file_path: &str) -> String {
        let path = Path::new(file_path);
        let mut parts = Vec::new();

        // Skip to after src/
        let path_str = path.to_string_lossy();
        let after_src: Vec<&str> = path_str.split("src").collect();
        if after_src.len() > 1 {
            for component in after_src[1].split(['/', '\\']) {
                if !component.is_empty() && component != "mod.rs" && component != "lib.rs" {
                    // Remove .rs extension
                    let name = component.trim_end_matches(".rs");
                    if !name.is_empty() {
                        parts.push(name.to_string());
                    }
                }
            }
        }

        parts.join("::")
    }

    /// Resolves a wildcard import to all symbols in the module
    fn resolve_wildcard(&self, module_path: &str, index: &dyn CodeIndex) -> Result<Vec<Symbol>> {
        // This is a simplified implementation
        // In practice, you'd need to find all public symbols in the module
        let parts: Vec<&str> = module_path.split("::").collect();
        if let Some(module_name) = parts.last() {
            // Find the module and get its members
            return index.get_symbol_members(module_name);
        }
        Ok(Vec::new())
    }
}

/// Java import resolver
///
/// Handles Java imports including:
/// - `import java.util.HashMap;`
/// - `import java.util.*;`
/// - `import static java.lang.Math.PI;`
pub struct JavaImportResolver;

impl ImportResolver for JavaImportResolver {
    fn resolve(&self, import: &FileImport, index: &dyn CodeIndex) -> Result<Vec<Symbol>> {
        let path = match &import.imported_path {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        // For wildcard imports, need to find all types in the package
        if import.import_type == ImportType::Wildcard {
            return self.resolve_wildcard_package(path, index);
        }

        // For specific imports, extract the class name from the path
        if let Some(ref symbol_name) = import.imported_symbol {
            return Ok(index.find_definition(symbol_name)?);
        }

        // Extract the last part as the class name
        let parts: Vec<&str> = path.split('.').collect();
        if let Some(class_name) = parts.last() {
            return Ok(index.find_definition(class_name)?);
        }

        Ok(Vec::new())
    }

    fn compute_fqdn(&self, symbol: &Symbol, file_path: &str) -> String {
        // Java FQDN is package.ClassName.member
        let path = Path::new(file_path);

        let mut parts = Vec::new();

        // Try to extract package from path
        // Standard Java layout: src/main/java/com/example/Class.java
        let path_str = path.to_string_lossy();
        if let Some(java_idx) = path_str.find("java/").or_else(|| path_str.find("java\\")) {
            let after_java = &path_str[java_idx + 5..];
            // Get directory parts as package
            if let Some(parent) = Path::new(after_java).parent() {
                for component in parent.components() {
                    parts.push(component.as_os_str().to_string_lossy().to_string());
                }
            }
        }

        // Add parent class if exists
        if let Some(ref parent) = symbol.parent {
            parts.push(parent.clone());
        }

        parts.push(symbol.name.clone());
        parts.join(".")
    }

    fn language(&self) -> &'static str {
        "java"
    }
}

impl JavaImportResolver {
    fn resolve_wildcard_package(&self, _package_path: &str, _index: &dyn CodeIndex) -> Result<Vec<Symbol>> {
        // This would need to find all public types in the package
        // Simplified implementation
        Ok(Vec::new())
    }
}

/// TypeScript/JavaScript import resolver
///
/// Handles ES imports including:
/// - `import { foo } from './module';`
/// - `import * as bar from 'package';`
/// - `import default from './module';`
/// - `const { foo } = require('./module');`
pub struct TypeScriptImportResolver;

impl ImportResolver for TypeScriptImportResolver {
    fn resolve(&self, import: &FileImport, index: &dyn CodeIndex) -> Result<Vec<Symbol>> {
        let path = match &import.imported_path {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        // Resolve relative imports
        let resolved_path = self.resolve_import_path(path, &import.file_path);

        // For named imports
        if let Some(ref symbol_name) = import.imported_symbol {
            return Ok(index.find_definition(symbol_name)?);
        }

        // For namespace imports, find all exports
        if import.import_type == ImportType::Wildcard {
            // Find all exported symbols from the module
            // This is simplified - would need module analysis
            return Ok(Vec::new());
        }

        // Try to find default export
        let default_names = ["default", "exports"];
        for name in default_names {
            let symbols = index.find_definition(name)?;
            let filtered: Vec<Symbol> = symbols
                .into_iter()
                .filter(|s| s.location.file_path.contains(&resolved_path))
                .collect();
            if !filtered.is_empty() {
                return Ok(filtered);
            }
        }

        Ok(Vec::new())
    }

    fn compute_fqdn(&self, symbol: &Symbol, file_path: &str) -> String {
        // TypeScript uses module paths
        let path = Path::new(file_path);

        let mut parts = Vec::new();

        // Extract path relative to src/
        let path_str = path.to_string_lossy();
        let after_src: Vec<&str> = path_str.split("src").collect();
        if after_src.len() > 1 {
            let rel_path = after_src[1].trim_start_matches(['/', '\\']);
            // Remove extension
            let module_path = rel_path
                .trim_end_matches(".ts")
                .trim_end_matches(".tsx")
                .trim_end_matches(".js")
                .trim_end_matches(".jsx");
            // Convert slashes to dots
            for part in module_path.split(['/', '\\']) {
                if !part.is_empty() && part != "index" {
                    parts.push(part.to_string());
                }
            }
        }

        // Add parent if exists
        if let Some(ref parent) = symbol.parent {
            parts.push(parent.clone());
        }

        parts.push(symbol.name.clone());
        parts.join(".")
    }

    fn language(&self) -> &'static str {
        "typescript"
    }
}

impl TypeScriptImportResolver {
    fn resolve_import_path(&self, import_path: &str, file_path: &str) -> String {
        if import_path.starts_with('.') {
            // Relative import
            let file_dir = Path::new(file_path).parent().unwrap_or(Path::new(""));
            let resolved = file_dir.join(import_path);
            resolved.to_string_lossy().to_string()
        } else {
            // Package import - use as-is
            import_path.to_string()
        }
    }
}

/// Python import resolver
///
/// Handles Python imports:
/// - `import module`
/// - `from package import module`
/// - `from package.subpackage import Class`
pub struct PythonImportResolver;

impl ImportResolver for PythonImportResolver {
    fn resolve(&self, import: &FileImport, index: &dyn CodeIndex) -> Result<Vec<Symbol>> {
        if let Some(ref symbol_name) = import.imported_symbol {
            return Ok(index.find_definition(symbol_name)?);
        }

        if let Some(ref path) = import.imported_path {
            let parts: Vec<&str> = path.split('.').collect();
            if let Some(module_name) = parts.last() {
                return Ok(index.find_definition(module_name)?);
            }
        }

        Ok(Vec::new())
    }

    fn compute_fqdn(&self, symbol: &Symbol, file_path: &str) -> String {
        let path = Path::new(file_path);
        let mut parts = Vec::new();

        // Python package is determined by __init__.py files
        // Simplified: use directory structure
        if let Some(parent) = path.parent() {
            for component in parent.components() {
                let name = component.as_os_str().to_string_lossy();
                if !name.is_empty() && name != "." && name != ".." {
                    parts.push(name.to_string());
                }
            }
        }

        // Add module name (file without .py)
        if let Some(stem) = path.file_stem() {
            let name = stem.to_string_lossy();
            if name != "__init__" {
                parts.push(name.to_string());
            }
        }

        if let Some(ref parent) = symbol.parent {
            parts.push(parent.clone());
        }

        parts.push(symbol.name.clone());
        parts.join(".")
    }

    fn language(&self) -> &'static str {
        "python"
    }
}

/// Go import resolver
pub struct GoImportResolver;

impl ImportResolver for GoImportResolver {
    fn resolve(&self, import: &FileImport, index: &dyn CodeIndex) -> Result<Vec<Symbol>> {
        if let Some(ref symbol_name) = import.imported_symbol {
            return Ok(index.find_definition(symbol_name)?);
        }

        if let Some(ref path) = import.imported_path {
            // Go import paths are package paths
            // Find exported symbols (capitalized) from the package
            let parts: Vec<&str> = path.split('/').collect();
            if let Some(package_name) = parts.last() {
                return Ok(index.find_definition(package_name)?);
            }
        }

        Ok(Vec::new())
    }

    fn compute_fqdn(&self, symbol: &Symbol, _file_path: &str) -> String {
        // Go uses package.Symbol
        if let Some(ref parent) = symbol.parent {
            format!("{}.{}", parent, symbol.name)
        } else {
            symbol.name.clone()
        }
    }

    fn language(&self) -> &'static str {
        "go"
    }
}

/// Registry of import resolvers
pub struct ImportResolverRegistry {
    resolvers: Vec<Box<dyn ImportResolver>>,
}

impl Default for ImportResolverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ImportResolverRegistry {
    pub fn new() -> Self {
        Self {
            resolvers: vec![
                Box::new(RustImportResolver),
                Box::new(JavaImportResolver),
                Box::new(TypeScriptImportResolver),
                Box::new(PythonImportResolver),
                Box::new(GoImportResolver),
            ],
        }
    }

    pub fn get(&self, language: &str) -> Option<&dyn ImportResolver> {
        self.resolvers.iter().find(|r| r.language() == language).map(|r| r.as_ref())
    }

    pub fn resolve(&self, import: &FileImport, language: &str, index: &dyn CodeIndex) -> Result<Vec<Symbol>> {
        if let Some(resolver) = self.get(language) {
            resolver.resolve(import, index)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn compute_fqdn(&self, symbol: &Symbol, file_path: &str, language: &str) -> String {
        if let Some(resolver) = self.get(language) {
            resolver.compute_fqdn(symbol, file_path)
        } else {
            symbol.name.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{Location, SymbolKind};

    // ============================================================================
    // Rust Import Resolver Tests
    // ============================================================================

    #[test]
    fn test_rust_file_to_module_path() {
        let resolver = RustImportResolver;
        assert_eq!(
            resolver.file_to_module_path("src/index/models.rs"),
            "index::models"
        );
        assert_eq!(resolver.file_to_module_path("src/lib.rs"), "");
    }

    #[test]
    fn test_rust_normalize_crate_path() {
        let resolver = RustImportResolver;
        let normalized = resolver.normalize_rust_path("crate::index::Symbol", "src/main.rs");
        assert_eq!(normalized, "crate::index::Symbol");
    }

    #[test]
    fn test_rust_normalize_self_path() {
        let resolver = RustImportResolver;
        let normalized = resolver.normalize_rust_path("self::submodule::Item", "src/index/mod.rs");
        assert_eq!(normalized, "index::submodule::Item");
    }

    #[test]
    fn test_rust_normalize_super_path() {
        let resolver = RustImportResolver;
        let normalized =
            resolver.normalize_rust_path("super::sibling::func", "src/index/models.rs");
        assert_eq!(normalized, "index::sibling::func");
    }

    #[test]
    fn test_rust_fqdn_function() {
        let resolver = RustImportResolver;
        let symbol = Symbol::new(
            "parse",
            SymbolKind::Function,
            Location::new("src/parser/mod.rs", 10, 0, 20, 1),
            "rust",
        );

        let fqdn = resolver.compute_fqdn(&symbol, "src/parser/mod.rs");
        assert_eq!(fqdn, "parser::parse");
    }

    #[test]
    fn test_rust_fqdn_struct_method() {
        let resolver = RustImportResolver;
        let mut symbol = Symbol::new(
            "new",
            SymbolKind::Method,
            Location::new("src/models.rs", 10, 0, 15, 1),
            "rust",
        );
        symbol.parent = Some("Config".to_string());

        let fqdn = resolver.compute_fqdn(&symbol, "src/models.rs");
        assert_eq!(fqdn, "models::Config::new");
    }

    #[test]
    fn test_rust_language() {
        let resolver = RustImportResolver;
        assert_eq!(resolver.language(), "rust");
    }

    // ============================================================================
    // Java Import Resolver Tests
    // ============================================================================

    #[test]
    fn test_java_fqdn() {
        let resolver = JavaImportResolver;
        let symbol = Symbol::new(
            "MyClass",
            SymbolKind::Class,
            Location::new("src/main/java/com/example/MyClass.java", 1, 0, 10, 1),
            "java",
        );

        let fqdn = resolver.compute_fqdn(&symbol, "src/main/java/com/example/MyClass.java");
        assert_eq!(fqdn, "com.example.MyClass");
    }

    #[test]
    fn test_java_fqdn_nested_class() {
        let resolver = JavaImportResolver;
        let mut symbol = Symbol::new(
            "Builder",
            SymbolKind::Class,
            Location::new("src/main/java/com/example/Config.java", 20, 0, 40, 1),
            "java",
        );
        symbol.parent = Some("Config".to_string());

        let fqdn = resolver.compute_fqdn(&symbol, "src/main/java/com/example/Config.java");
        assert_eq!(fqdn, "com.example.Config.Builder");
    }

    #[test]
    fn test_java_language() {
        let resolver = JavaImportResolver;
        assert_eq!(resolver.language(), "java");
    }

    // ============================================================================
    // TypeScript Import Resolver Tests
    // ============================================================================

    #[test]
    fn test_typescript_resolve_relative_path() {
        let resolver = TypeScriptImportResolver;
        let resolved = resolver.resolve_import_path("./utils", "src/components/Button.tsx");
        assert!(resolved.contains("components"));
    }

    #[test]
    fn test_typescript_resolve_package_import() {
        let resolver = TypeScriptImportResolver;
        let resolved = resolver.resolve_import_path("react", "src/components/Button.tsx");
        assert_eq!(resolved, "react");
    }

    #[test]
    fn test_typescript_fqdn() {
        let resolver = TypeScriptImportResolver;
        let symbol = Symbol::new(
            "Button",
            SymbolKind::Function,
            Location::new("src/components/Button.tsx", 1, 0, 50, 1),
            "typescript",
        );

        let fqdn = resolver.compute_fqdn(&symbol, "src/components/Button.tsx");
        assert_eq!(fqdn, "components.Button.Button");
    }

    #[test]
    fn test_typescript_fqdn_class_method() {
        let resolver = TypeScriptImportResolver;
        let mut symbol = Symbol::new(
            "render",
            SymbolKind::Method,
            Location::new("src/components/Widget.tsx", 10, 0, 20, 1),
            "typescript",
        );
        symbol.parent = Some("Widget".to_string());

        let fqdn = resolver.compute_fqdn(&symbol, "src/components/Widget.tsx");
        assert_eq!(fqdn, "components.Widget.Widget.render");
    }

    #[test]
    fn test_typescript_language() {
        let resolver = TypeScriptImportResolver;
        assert_eq!(resolver.language(), "typescript");
    }

    // ============================================================================
    // Python Import Resolver Tests
    // ============================================================================

    #[test]
    fn test_python_fqdn() {
        let resolver = PythonImportResolver;
        let symbol = Symbol::new(
            "MyClass",
            SymbolKind::Class,
            Location::new("src/models/user.py", 1, 0, 20, 1),
            "python",
        );

        let fqdn = resolver.compute_fqdn(&symbol, "src/models/user.py");
        assert!(fqdn.ends_with("user.MyClass"));
    }

    #[test]
    fn test_python_fqdn_init_file() {
        let resolver = PythonImportResolver;
        let symbol = Symbol::new(
            "initialize",
            SymbolKind::Function,
            Location::new("src/models/__init__.py", 1, 0, 10, 1),
            "python",
        );

        let fqdn = resolver.compute_fqdn(&symbol, "src/models/__init__.py");
        // Should not include __init__ in path
        assert!(fqdn.ends_with("initialize"));
        assert!(!fqdn.contains("__init__"));
    }

    #[test]
    fn test_python_fqdn_class_method() {
        let resolver = PythonImportResolver;
        let mut symbol = Symbol::new(
            "save",
            SymbolKind::Method,
            Location::new("src/models/user.py", 10, 0, 15, 1),
            "python",
        );
        symbol.parent = Some("User".to_string());

        let fqdn = resolver.compute_fqdn(&symbol, "src/models/user.py");
        assert!(fqdn.ends_with("User.save"));
    }

    #[test]
    fn test_python_language() {
        let resolver = PythonImportResolver;
        assert_eq!(resolver.language(), "python");
    }

    // ============================================================================
    // Go Import Resolver Tests
    // ============================================================================

    #[test]
    fn test_go_fqdn_function() {
        let resolver = GoImportResolver;
        let symbol = Symbol::new(
            "ParseConfig",
            SymbolKind::Function,
            Location::new("pkg/config/parser.go", 10, 0, 30, 1),
            "go",
        );

        let fqdn = resolver.compute_fqdn(&symbol, "pkg/config/parser.go");
        assert_eq!(fqdn, "ParseConfig");
    }

    #[test]
    fn test_go_fqdn_method() {
        let resolver = GoImportResolver;
        let mut symbol = Symbol::new(
            "Parse",
            SymbolKind::Method,
            Location::new("pkg/config/parser.go", 10, 0, 30, 1),
            "go",
        );
        symbol.parent = Some("Parser".to_string());

        let fqdn = resolver.compute_fqdn(&symbol, "pkg/config/parser.go");
        assert_eq!(fqdn, "Parser.Parse");
    }

    #[test]
    fn test_go_language() {
        let resolver = GoImportResolver;
        assert_eq!(resolver.language(), "go");
    }

    // ============================================================================
    // Registry Tests
    // ============================================================================

    #[test]
    fn test_registry_default() {
        let registry = ImportResolverRegistry::default();
        assert!(registry.get("rust").is_some());
        assert!(registry.get("java").is_some());
        assert!(registry.get("typescript").is_some());
        assert!(registry.get("python").is_some());
        assert!(registry.get("go").is_some());
    }

    #[test]
    fn test_registry_unknown_language() {
        let registry = ImportResolverRegistry::new();
        assert!(registry.get("unknown").is_none());
    }

    #[test]
    fn test_registry_compute_fqdn_fallback() {
        let registry = ImportResolverRegistry::new();
        let symbol = Symbol::new(
            "test",
            SymbolKind::Function,
            Location::new("test.xyz", 1, 0, 5, 1),
            "unknown",
        );

        // Unknown language returns symbol name
        let fqdn = registry.compute_fqdn(&symbol, "test.xyz", "unknown");
        assert_eq!(fqdn, "test");
    }
}
