pub mod cross_language;
pub mod java;
pub mod kotlin;
pub mod rust;
pub mod typescript;
pub mod python;
pub mod go;
pub mod csharp;
pub mod cpp;

pub use cross_language::{CrossLanguageAnalyzer, CrossLanguageRef, CrossRefType};

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tree_sitter::Query;

pub trait LanguageGrammar: Send + Sync {
    fn name(&self) -> &'static str;
    fn file_extensions(&self) -> &[&'static str];
    fn language(&self) -> tree_sitter::Language;
    fn functions_query(&self) -> &str;
    fn types_query(&self) -> &str;
    fn imports_query(&self) -> &str;

    /// Query for extracting references (function calls, type usages, etc.)
    fn references_query(&self) -> &str {
        ""
    }

    /// Get cached functions query (compiled once)
    fn cached_functions_query(&self) -> Option<&'static Query> {
        None
    }

    /// Get cached types query (compiled once)
    fn cached_types_query(&self) -> Option<&'static Query> {
        None
    }

    /// Get cached imports query (compiled once)
    fn cached_imports_query(&self) -> Option<&'static Query> {
        None
    }

    /// Get cached references query (compiled once)
    fn cached_references_query(&self) -> Option<&'static Query> {
        None
    }
}

pub struct LanguageRegistry {
    languages: HashMap<String, Arc<dyn LanguageGrammar>>,
    extension_map: HashMap<String, String>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            languages: HashMap::new(),
            extension_map: HashMap::new(),
        };

        registry.register(Arc::new(rust::RustGrammar));
        registry.register(Arc::new(java::JavaGrammar));
        registry.register(Arc::new(kotlin::KotlinGrammar));
        registry.register(Arc::new(typescript::TypeScriptGrammar));
        registry.register(Arc::new(python::PythonGrammar));
        registry.register(Arc::new(go::GoGrammar));
        registry.register(Arc::new(csharp::CSharpGrammar));
        registry.register(Arc::new(cpp::CppGrammar));

        registry
    }

    pub fn register(&mut self, grammar: Arc<dyn LanguageGrammar>) {
        let name = grammar.name().to_string();
        for ext in grammar.file_extensions() {
            self.extension_map.insert(ext.to_string(), name.clone());
        }
        self.languages.insert(name, grammar);
    }

    #[allow(dead_code)]
    pub fn get_by_name(&self, name: &str) -> Option<Arc<dyn LanguageGrammar>> {
        self.languages.get(name).cloned()
    }

    pub fn get_by_extension(&self, ext: &str) -> Option<Arc<dyn LanguageGrammar>> {
        self.extension_map
            .get(ext)
            .and_then(|name| self.languages.get(name))
            .cloned()
    }

    pub fn get_for_file(&self, path: &Path) -> Option<Arc<dyn LanguageGrammar>> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| self.get_by_extension(ext))
    }

    #[allow(dead_code)]
    pub fn supported_extensions(&self) -> Vec<&str> {
        self.extension_map.keys().map(|s| s.as_str()).collect()
    }

    #[allow(dead_code)]
    pub fn supported_languages(&self) -> Vec<&str> {
        self.languages.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let registry = LanguageRegistry::new();
        assert!(registry.get_by_name("rust").is_some());
        assert!(registry.get_by_name("java").is_some());
        assert!(registry.get_by_name("kotlin").is_some());
        assert!(registry.get_by_name("typescript").is_some());
    }

    #[test]
    fn test_registry_default() {
        let registry = LanguageRegistry::default();
        assert!(registry.get_by_name("rust").is_some());
    }

    #[test]
    fn test_get_by_name_rust() {
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("rust").unwrap();
        assert_eq!(grammar.name(), "rust");
    }

    #[test]
    fn test_get_by_name_java() {
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("java").unwrap();
        assert_eq!(grammar.name(), "java");
    }

    #[test]
    fn test_get_by_name_typescript() {
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("typescript").unwrap();
        assert_eq!(grammar.name(), "typescript");
    }

    #[test]
    fn test_get_by_name_kotlin() {
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_name("kotlin").unwrap();
        assert_eq!(grammar.name(), "kotlin");
    }

    #[test]
    fn test_get_by_name_unknown() {
        let registry = LanguageRegistry::new();
        assert!(registry.get_by_name("unknown_lang").is_none());
        assert!(registry.get_by_name("").is_none());
    }

    #[test]
    fn test_get_by_extension_rs() {
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_extension("rs").unwrap();
        assert_eq!(grammar.name(), "rust");
    }

    #[test]
    fn test_get_by_extension_java() {
        let registry = LanguageRegistry::new();
        let grammar = registry.get_by_extension("java").unwrap();
        assert_eq!(grammar.name(), "java");
    }

    #[test]
    fn test_get_by_extension_typescript() {
        let registry = LanguageRegistry::new();

        let ts = registry.get_by_extension("ts").unwrap();
        assert_eq!(ts.name(), "typescript");

        let tsx = registry.get_by_extension("tsx").unwrap();
        assert_eq!(tsx.name(), "typescript");

        let js = registry.get_by_extension("js").unwrap();
        assert_eq!(js.name(), "typescript");

        let jsx = registry.get_by_extension("jsx").unwrap();
        assert_eq!(jsx.name(), "typescript");
    }

    #[test]
    fn test_get_by_extension_kotlin() {
        let registry = LanguageRegistry::new();

        let kt = registry.get_by_extension("kt").unwrap();
        assert_eq!(kt.name(), "kotlin");

        let kts = registry.get_by_extension("kts").unwrap();
        assert_eq!(kts.name(), "kotlin");
    }

    #[test]
    fn test_get_by_extension_unknown() {
        let registry = LanguageRegistry::new();
        assert!(registry.get_by_extension("unknown").is_none());
        assert!(registry.get_by_extension("xyz").is_none());
        assert!(registry.get_by_extension("").is_none());
    }

    #[test]
    fn test_get_by_extension_python() {
        let registry = LanguageRegistry::new();
        let py = registry.get_by_extension("py").unwrap();
        assert_eq!(py.name(), "python");
    }

    #[test]
    fn test_get_by_extension_go() {
        let registry = LanguageRegistry::new();
        let go = registry.get_by_extension("go").unwrap();
        assert_eq!(go.name(), "go");
    }

    #[test]
    fn test_get_by_extension_csharp() {
        let registry = LanguageRegistry::new();
        let cs = registry.get_by_extension("cs").unwrap();
        assert_eq!(cs.name(), "csharp");
    }

    #[test]
    fn test_get_by_extension_cpp() {
        let registry = LanguageRegistry::new();
        let cpp = registry.get_by_extension("cpp").unwrap();
        assert_eq!(cpp.name(), "cpp");

        let h = registry.get_by_extension("h").unwrap();
        assert_eq!(h.name(), "cpp");
    }

    #[test]
    fn test_get_for_file_rust() {
        let registry = LanguageRegistry::new();
        let grammar = registry.get_for_file(Path::new("src/main.rs")).unwrap();
        assert_eq!(grammar.name(), "rust");
    }

    #[test]
    fn test_get_for_file_java() {
        let registry = LanguageRegistry::new();
        let grammar = registry.get_for_file(Path::new("com/example/Main.java")).unwrap();
        assert_eq!(grammar.name(), "java");
    }

    #[test]
    fn test_get_for_file_typescript() {
        let registry = LanguageRegistry::new();

        let ts = registry.get_for_file(Path::new("app.ts")).unwrap();
        assert_eq!(ts.name(), "typescript");

        let tsx = registry.get_for_file(Path::new("Component.tsx")).unwrap();
        assert_eq!(tsx.name(), "typescript");
    }

    #[test]
    fn test_get_for_file_kotlin() {
        let registry = LanguageRegistry::new();

        let kt = registry.get_for_file(Path::new("com/example/Main.kt")).unwrap();
        assert_eq!(kt.name(), "kotlin");

        let kts = registry.get_for_file(Path::new("build.gradle.kts")).unwrap();
        assert_eq!(kts.name(), "kotlin");
    }

    #[test]
    fn test_get_for_file_no_extension() {
        let registry = LanguageRegistry::new();
        assert!(registry.get_for_file(Path::new("Makefile")).is_none());
    }

    #[test]
    fn test_get_for_file_unknown_extension() {
        let registry = LanguageRegistry::new();
        assert!(registry.get_for_file(Path::new("file.txt")).is_none());
        assert!(registry.get_for_file(Path::new("data.json")).is_none());
    }

    #[test]
    fn test_get_for_file_python() {
        let registry = LanguageRegistry::new();
        let py = registry.get_for_file(Path::new("script.py")).unwrap();
        assert_eq!(py.name(), "python");
    }

    #[test]
    fn test_get_for_file_go() {
        let registry = LanguageRegistry::new();
        let go = registry.get_for_file(Path::new("main.go")).unwrap();
        assert_eq!(go.name(), "go");
    }

    #[test]
    fn test_supported_extensions() {
        let registry = LanguageRegistry::new();
        let exts = registry.supported_extensions();

        assert!(exts.contains(&"rs"));
        assert!(exts.contains(&"java"));
        assert!(exts.contains(&"kt"));
        assert!(exts.contains(&"kts"));
        assert!(exts.contains(&"ts"));
        assert!(exts.contains(&"tsx"));
        assert!(exts.contains(&"js"));
        assert!(exts.contains(&"jsx"));
    }

    #[test]
    fn test_supported_languages() {
        let registry = LanguageRegistry::new();
        let langs = registry.supported_languages();

        assert!(langs.contains(&"rust"));
        assert!(langs.contains(&"java"));
        assert!(langs.contains(&"kotlin"));
        assert!(langs.contains(&"typescript"));
        assert!(langs.contains(&"python"));
        assert!(langs.contains(&"go"));
        assert!(langs.contains(&"csharp"));
        assert!(langs.contains(&"cpp"));
        assert_eq!(langs.len(), 8);
    }

    #[test]
    fn test_grammar_functions_query_not_empty() {
        let registry = LanguageRegistry::new();

        let rust = registry.get_by_name("rust").unwrap();
        assert!(!rust.functions_query().is_empty());

        let java = registry.get_by_name("java").unwrap();
        assert!(!java.functions_query().is_empty());

        let kotlin = registry.get_by_name("kotlin").unwrap();
        assert!(!kotlin.functions_query().is_empty());

        let ts = registry.get_by_name("typescript").unwrap();
        assert!(!ts.functions_query().is_empty());
    }

    #[test]
    fn test_grammar_types_query_not_empty() {
        let registry = LanguageRegistry::new();

        let rust = registry.get_by_name("rust").unwrap();
        assert!(!rust.types_query().is_empty());

        let java = registry.get_by_name("java").unwrap();
        assert!(!java.types_query().is_empty());

        let kotlin = registry.get_by_name("kotlin").unwrap();
        assert!(!kotlin.types_query().is_empty());

        let ts = registry.get_by_name("typescript").unwrap();
        assert!(!ts.types_query().is_empty());
    }
}
