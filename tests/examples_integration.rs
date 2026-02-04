//! Integration tests for example projects.
//!
//! These tests verify that the indexer correctly parses and indexes
//! code from each example project (Rust, Java, Kotlin, TypeScript).

use std::path::PathBuf;

use code_indexer::{
    CodeIndex, FileWalker, LanguageRegistry, Parser, SearchOptions, SqliteIndex, SymbolExtractor,
    SymbolKind,
};
use tempfile::NamedTempFile;

/// Helper to create an index from a directory
fn index_directory(path: &str) -> SqliteIndex {
    let temp_db = NamedTempFile::new().expect("Failed to create temp file");
    let db_path = temp_db.path().to_path_buf();

    // Keep the temp file alive by leaking it (it will be cleaned up when process exits)
    let _ = temp_db.into_temp_path();

    let registry = LanguageRegistry::new();
    let walker = FileWalker::new(registry);
    let registry = LanguageRegistry::new();
    let parser = Parser::new(registry);
    let extractor = SymbolExtractor::new();
    let index = SqliteIndex::new(&db_path).expect("Failed to create index");

    let base_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    let files = walker.walk(&base_path).expect("Failed to walk directory");

    for file in &files {
        if let Ok(parsed) = parser.parse_file(file) {
            if let Ok(symbols) = extractor.extract(&parsed, file) {
                index.add_symbols(symbols).expect("Failed to add symbols");
            }
        }
    }

    index
}

// ============================================================================
// Rust Example Tests
// ============================================================================

mod rust_example {
    use super::*;

    fn get_index() -> SqliteIndex {
        index_directory("examples/rust-example")
    }

    #[test]
    fn test_index_has_symbols() {
        let index = get_index();
        let stats = index.get_stats().unwrap();

        assert!(stats.total_symbols > 0, "Index should have symbols");
        assert!(stats.total_files > 0, "Index should have files");
    }

    #[test]
    fn test_find_repository_trait() {
        let index = get_index();
        let results = index.find_definition("Repository").unwrap();

        assert!(!results.is_empty(), "Should find Repository trait");

        let repo = results.iter().find(|s| s.kind == SymbolKind::Trait);
        assert!(repo.is_some(), "Repository should be a trait");
    }

    #[test]
    fn test_find_user_struct() {
        let index = get_index();
        let results = index.find_definition("User").unwrap();

        assert!(!results.is_empty(), "Should find User struct");

        let user = results.iter().find(|s| s.kind == SymbolKind::Struct);
        assert!(user.is_some(), "User should be a struct");
    }

    #[test]
    fn test_find_product_struct() {
        let index = get_index();
        let results = index.find_definition("Product").unwrap();

        assert!(!results.is_empty(), "Should find Product struct");

        let product = results.iter().find(|s| s.kind == SymbolKind::Struct);
        assert!(product.is_some(), "Product should be a struct");
    }

    #[test]
    fn test_find_status_enum() {
        let index = get_index();
        let results = index.find_definition("Status").unwrap();

        assert!(!results.is_empty(), "Should find Status enum");

        let status = results.iter().find(|s| s.kind == SymbolKind::Enum);
        assert!(status.is_some(), "Status should be an enum");
    }

    #[test]
    fn test_find_inmemory_user_repository() {
        let index = get_index();
        let results = index.find_definition("InMemoryUserRepository").unwrap();

        assert!(!results.is_empty(), "Should find InMemoryUserRepository");
    }

    #[test]
    fn test_search_repository_implementations() {
        let index = get_index();
        let options = SearchOptions {
            limit: Some(20),
            ..Default::default()
        };
        let results = index.search("Repository", &options).unwrap();

        assert!(results.len() >= 4, "Should find multiple Repository-related symbols");

        let names: Vec<_> = results.iter().map(|r| r.symbol.name.as_str()).collect();
        assert!(names.contains(&"Repository"), "Should find Repository trait");
        assert!(
            names.iter().any(|n| n.contains("UserRepository")),
            "Should find UserRepository implementations"
        );
    }

    #[test]
    fn test_list_functions_rust() {
        let index = get_index();
        let options = SearchOptions {
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(100),
            ..Default::default()
        };
        let functions = index.list_functions(&options).unwrap();

        assert!(!functions.is_empty(), "Should have Rust functions");

        let names: Vec<_> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"new"), "Should have new() function");
        assert!(names.contains(&"save"), "Should have save() function");
        assert!(names.contains(&"find_by_id"), "Should have find_by_id() function");
    }

    #[test]
    fn test_list_types_rust() {
        let index = get_index();
        let options = SearchOptions {
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(50),
            ..Default::default()
        };
        let types = index.list_types(&options).unwrap();

        assert!(!types.is_empty(), "Should have Rust types");

        let names: Vec<_> = types.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"User"), "Should have User type");
        assert!(names.contains(&"Product"), "Should have Product type");
        assert!(names.contains(&"Status"), "Should have Status type");
        assert!(names.contains(&"Repository"), "Should have Repository trait");
    }
}

// ============================================================================
// Java Maven Example Tests
// ============================================================================

mod java_maven_example {
    use super::*;

    fn get_index() -> SqliteIndex {
        index_directory("examples/java-maven")
    }

    #[test]
    fn test_index_has_symbols() {
        let index = get_index();
        let stats = index.get_stats().unwrap();

        assert!(stats.total_symbols > 0, "Index should have symbols");
        assert!(stats.total_files > 0, "Index should have files");
    }

    #[test]
    fn test_find_repository_interface() {
        let index = get_index();
        let results = index.find_definition("Repository").unwrap();

        assert!(!results.is_empty(), "Should find Repository interface");

        let repo = results.iter().find(|s| s.kind == SymbolKind::Interface);
        assert!(repo.is_some(), "Repository should be an interface");
    }

    #[test]
    fn test_find_user_class() {
        let index = get_index();
        let results = index.find_definition("User").unwrap();

        assert!(!results.is_empty(), "Should find User class");

        let user = results.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(user.is_some(), "User should be a class");
    }

    #[test]
    fn test_find_product_class() {
        let index = get_index();
        let results = index.find_definition("Product").unwrap();

        assert!(!results.is_empty(), "Should find Product class");
    }

    #[test]
    fn test_find_status_enum() {
        let index = get_index();
        let results = index.find_definition("Status").unwrap();

        assert!(!results.is_empty(), "Should find Status enum");

        let status = results.iter().find(|s| s.kind == SymbolKind::Enum);
        assert!(status.is_some(), "Status should be an enum");
    }

    #[test]
    fn test_find_abstract_entity() {
        let index = get_index();
        let results = index.find_definition("AbstractEntity").unwrap();

        assert!(!results.is_empty(), "Should find AbstractEntity");
    }

    #[test]
    fn test_find_inmemory_user_repository() {
        let index = get_index();
        let results = index.find_definition("InMemoryUserRepository").unwrap();

        assert!(!results.is_empty(), "Should find InMemoryUserRepository");
    }

    #[test]
    fn test_search_validator() {
        let index = get_index();
        let options = SearchOptions::default();
        let results = index.search("Validator", &options).unwrap();

        assert!(!results.is_empty(), "Should find Validator");
    }

    #[test]
    fn test_list_functions_java() {
        let index = get_index();
        let options = SearchOptions {
            language_filter: Some(vec!["java".to_string()]),
            limit: Some(100),
            ..Default::default()
        };
        let functions = index.list_functions(&options).unwrap();

        assert!(!functions.is_empty(), "Should have Java functions");

        let names: Vec<_> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"save"), "Should have save() method");
        assert!(names.contains(&"findById"), "Should have findById() method");
        assert!(names.contains(&"findAll"), "Should have findAll() method");
    }

    #[test]
    fn test_list_types_java() {
        let index = get_index();
        let options = SearchOptions {
            language_filter: Some(vec!["java".to_string()]),
            limit: Some(50),
            ..Default::default()
        };
        let types = index.list_types(&options).unwrap();

        assert!(!types.is_empty(), "Should have Java types");

        let names: Vec<_> = types.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"Repository"), "Should have Repository interface");
        assert!(names.contains(&"User"), "Should have User class");
        assert!(names.contains(&"Product"), "Should have Product class");
        assert!(names.contains(&"Status"), "Should have Status enum");
    }
}

// ============================================================================
// Java Gradle Example Tests
// ============================================================================

mod java_gradle_example {
    use super::*;

    fn get_index() -> SqliteIndex {
        index_directory("examples/java-gradle")
    }

    #[test]
    fn test_index_has_symbols() {
        let index = get_index();
        let stats = index.get_stats().unwrap();

        assert!(stats.total_symbols > 0, "Index should have symbols");
    }

    #[test]
    fn test_find_repository_interface() {
        let index = get_index();
        let results = index.find_definition("Repository").unwrap();

        assert!(!results.is_empty(), "Should find Repository interface");
    }

    #[test]
    fn test_find_all_implementations() {
        let index = get_index();

        let user_repo = index.find_definition("InMemoryUserRepository").unwrap();
        assert!(!user_repo.is_empty(), "Should find InMemoryUserRepository");

        let file_user_repo = index.find_definition("FileUserRepository").unwrap();
        assert!(!file_user_repo.is_empty(), "Should find FileUserRepository");

        let product_repo = index.find_definition("InMemoryProductRepository").unwrap();
        assert!(!product_repo.is_empty(), "Should find InMemoryProductRepository");

        let file_product_repo = index.find_definition("FileProductRepository").unwrap();
        assert!(!file_product_repo.is_empty(), "Should find FileProductRepository");
    }

    #[test]
    fn test_search_abstract() {
        let index = get_index();
        let options = SearchOptions::default();
        let results = index.search("Abstract", &options).unwrap();

        assert!(!results.is_empty(), "Should find Abstract classes");

        let names: Vec<_> = results.iter().map(|r| r.symbol.name.as_str()).collect();
        assert!(
            names.iter().any(|n| n.contains("AbstractEntity")),
            "Should find AbstractEntity"
        );
        assert!(
            names.iter().any(|n| n.contains("AbstractRepository")),
            "Should find AbstractRepository"
        );
    }
}

// ============================================================================
// Kotlin Gradle Example Tests
// ============================================================================

mod kotlin_gradle_example {
    use super::*;

    fn get_index() -> SqliteIndex {
        index_directory("examples/kotlin-gradle")
    }

    #[test]
    fn test_index_has_symbols() {
        let index = get_index();
        let stats = index.get_stats().unwrap();

        assert!(stats.total_symbols > 0, "Index should have symbols");
        assert!(stats.total_files > 0, "Index should have files");
    }

    #[test]
    fn test_find_repository_interface() {
        let index = get_index();
        let results = index.find_definition("Repository").unwrap();

        assert!(!results.is_empty(), "Should find Repository interface");
    }

    #[test]
    fn test_find_user_data_class() {
        let index = get_index();
        let results = index.find_definition("User").unwrap();

        assert!(!results.is_empty(), "Should find User data class");

        let user = results.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(user.is_some(), "User should be a class");
    }

    #[test]
    fn test_find_product_data_class() {
        let index = get_index();
        let results = index.find_definition("Product").unwrap();

        assert!(!results.is_empty(), "Should find Product data class");
    }

    #[test]
    fn test_find_status_enum() {
        let index = get_index();
        let results = index.find_definition("Status").unwrap();

        assert!(!results.is_empty(), "Should find Status enum class");
    }

    #[test]
    fn test_find_sealed_class_result() {
        let index = get_index();
        let results = index.find_definition("Result").unwrap();

        assert!(!results.is_empty(), "Should find Result sealed class");
    }

    #[test]
    fn test_find_inmemory_repositories() {
        let index = get_index();

        let user_repo = index.find_definition("InMemoryUserRepository").unwrap();
        assert!(!user_repo.is_empty(), "Should find InMemoryUserRepository");

        let product_repo = index.find_definition("InMemoryProductRepository").unwrap();
        assert!(!product_repo.is_empty(), "Should find InMemoryProductRepository");
    }

    #[test]
    fn test_find_extension_function() {
        let index = get_index();
        let options = SearchOptions::default();
        let results = index.search("toDTO", &options).unwrap();

        assert!(!results.is_empty(), "Should find toDTO extension function");
    }

    #[test]
    fn test_find_type_alias() {
        let index = get_index();
        let results = index.find_definition("UserList").unwrap();

        assert!(!results.is_empty(), "Should find UserList type alias");
    }

    #[test]
    fn test_list_functions_kotlin() {
        let index = get_index();
        let options = SearchOptions {
            language_filter: Some(vec!["kotlin".to_string()]),
            limit: Some(100),
            ..Default::default()
        };
        let functions = index.list_functions(&options).unwrap();

        assert!(!functions.is_empty(), "Should have Kotlin functions");

        let names: Vec<_> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"main"), "Should have main() function");
        assert!(names.contains(&"save"), "Should have save() function");
        assert!(names.contains(&"findById"), "Should have findById() function");
    }

    #[test]
    fn test_list_types_kotlin() {
        let index = get_index();
        let options = SearchOptions {
            language_filter: Some(vec!["kotlin".to_string()]),
            limit: Some(50),
            ..Default::default()
        };
        let types = index.list_types(&options).unwrap();

        assert!(!types.is_empty(), "Should have Kotlin types");

        let names: Vec<_> = types.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"User"), "Should have User data class");
        assert!(names.contains(&"Product"), "Should have Product data class");
        assert!(names.contains(&"Repository"), "Should have Repository interface");
    }
}

// ============================================================================
// TypeScript Example Tests
// ============================================================================

mod typescript_example {
    use super::*;

    fn get_index() -> SqliteIndex {
        index_directory("examples/typescript-example")
    }

    #[test]
    fn test_index_has_symbols() {
        let index = get_index();
        let stats = index.get_stats().unwrap();

        assert!(stats.total_symbols > 0, "Index should have symbols");
        assert!(stats.total_files > 0, "Index should have files");
    }

    #[test]
    fn test_find_repository_interface() {
        let index = get_index();
        let results = index.find_definition("Repository").unwrap();

        assert!(!results.is_empty(), "Should find Repository interface");

        let repo = results.iter().find(|s| s.kind == SymbolKind::Interface);
        assert!(repo.is_some(), "Repository should be an interface");
    }

    #[test]
    fn test_find_user_class() {
        let index = get_index();
        let results = index.find_definition("User").unwrap();

        assert!(!results.is_empty(), "Should find User class");

        let user = results.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(user.is_some(), "User should be a class");
    }

    #[test]
    fn test_find_product_class() {
        let index = get_index();
        let results = index.find_definition("Product").unwrap();

        assert!(!results.is_empty(), "Should find Product class");
    }

    #[test]
    fn test_find_status_enum() {
        let index = get_index();
        let results = index.find_definition("Status").unwrap();

        assert!(!results.is_empty(), "Should find Status enum");

        let status = results.iter().find(|s| s.kind == SymbolKind::Enum);
        assert!(status.is_some(), "Status should be an enum");
    }

    #[test]
    fn test_find_type_alias_user_dto() {
        let index = get_index();
        let results = index.find_definition("UserDTO").unwrap();

        assert!(!results.is_empty(), "Should find UserDTO type alias");
    }

    #[test]
    fn test_find_type_guard_is_user() {
        let index = get_index();
        let options = SearchOptions::default();
        let results = index.search("isUser", &options).unwrap();

        assert!(!results.is_empty(), "Should find isUser type guard");
    }

    #[test]
    fn test_find_serializable_interface() {
        let index = get_index();
        let results = index.find_definition("Serializable").unwrap();

        assert!(!results.is_empty(), "Should find Serializable interface");
    }

    #[test]
    fn test_find_inmemory_user_repository() {
        let index = get_index();
        let results = index.find_definition("InMemoryUserRepository").unwrap();

        assert!(!results.is_empty(), "Should find InMemoryUserRepository");
    }

    #[test]
    fn test_search_localstorage() {
        let index = get_index();
        let options = SearchOptions::default();
        let results = index.search("LocalStorage", &options).unwrap();

        assert!(!results.is_empty(), "Should find LocalStorage repositories");

        let names: Vec<_> = results.iter().map(|r| r.symbol.name.as_str()).collect();
        assert!(
            names.iter().any(|n| n.contains("LocalStorageUserRepository")),
            "Should find LocalStorageUserRepository"
        );
    }

    #[test]
    fn test_list_functions_typescript() {
        let index = get_index();
        let options = SearchOptions {
            language_filter: Some(vec!["typescript".to_string()]),
            limit: Some(100),
            ..Default::default()
        };
        let functions = index.list_functions(&options).unwrap();

        assert!(!functions.is_empty(), "Should have TypeScript functions");

        let names: Vec<_> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"main"), "Should have main() function");
        assert!(names.contains(&"save"), "Should have save() method");
        assert!(names.contains(&"findById"), "Should have findById() method");
        assert!(names.contains(&"delete"), "Should have delete() method");
    }

    #[test]
    fn test_list_types_typescript() {
        let index = get_index();
        let options = SearchOptions {
            language_filter: Some(vec!["typescript".to_string()]),
            limit: Some(50),
            ..Default::default()
        };
        let types = index.list_types(&options).unwrap();

        assert!(!types.is_empty(), "Should have TypeScript types");

        let names: Vec<_> = types.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"Repository"), "Should have Repository interface");
        assert!(names.contains(&"User"), "Should have User class");
        assert!(names.contains(&"Product"), "Should have Product class");
        assert!(names.contains(&"Status"), "Should have Status enum");
        assert!(names.contains(&"UserDTO"), "Should have UserDTO type alias");
    }
}

// ============================================================================
// Cross-Language Tests
// ============================================================================

mod cross_language {
    use super::*;

    fn get_all_examples_index() -> SqliteIndex {
        index_directory("examples")
    }

    #[test]
    fn test_all_languages_indexed() {
        let index = get_all_examples_index();
        let stats = index.get_stats().unwrap();

        let languages: Vec<_> = stats.symbols_by_language.iter().map(|(l, _)| l.as_str()).collect();

        assert!(
            languages.contains(&"rust"),
            "Should have Rust symbols"
        );
        assert!(
            languages.contains(&"java"),
            "Should have Java symbols"
        );
        assert!(
            languages.contains(&"kotlin"),
            "Should have Kotlin symbols"
        );
        assert!(
            languages.contains(&"typescript"),
            "Should have TypeScript symbols"
        );
    }

    #[test]
    fn test_repository_in_all_languages() {
        let index = get_all_examples_index();
        let results = index.find_definition("Repository").unwrap();

        // Should find Repository in Rust (trait), Java (interface), Kotlin (interface), TypeScript (interface)
        assert!(
            results.len() >= 4,
            "Should find Repository in at least 4 languages, found {}",
            results.len()
        );

        let languages: Vec<_> = results.iter().map(|r| r.language.as_str()).collect();
        assert!(languages.contains(&"rust"), "Should have Rust Repository");
        assert!(languages.contains(&"java"), "Should have Java Repository");
        assert!(languages.contains(&"kotlin"), "Should have Kotlin Repository");
        assert!(
            languages.contains(&"typescript"),
            "Should have TypeScript Repository"
        );
    }

    #[test]
    fn test_user_in_all_languages() {
        let index = get_all_examples_index();
        let results = index.find_definition("User").unwrap();

        assert!(
            results.len() >= 4,
            "Should find User in at least 4 languages"
        );

        let languages: Vec<_> = results.iter().map(|r| r.language.as_str()).collect();
        assert!(languages.contains(&"rust"), "Should have Rust User");
        assert!(languages.contains(&"java"), "Should have Java User");
        assert!(languages.contains(&"kotlin"), "Should have Kotlin User");
        assert!(
            languages.contains(&"typescript"),
            "Should have TypeScript User"
        );
    }

    #[test]
    fn test_search_across_languages() {
        let index = get_all_examples_index();
        let options = SearchOptions {
            limit: Some(50),
            ..Default::default()
        };
        let results = index.search("InMemory", &options).unwrap();

        // Should find InMemoryUserRepository and InMemoryProductRepository in multiple languages
        assert!(results.len() >= 8, "Should find InMemory* in multiple languages");

        let languages: std::collections::HashSet<_> =
            results.iter().map(|r| r.symbol.language.as_str()).collect();
        assert!(languages.len() >= 4, "Should span at least 4 languages");
    }

    #[test]
    fn test_filter_by_language() {
        let index = get_all_examples_index();

        // Rust only
        let rust_options = SearchOptions {
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(100),
            ..Default::default()
        };
        let rust_types = index.list_types(&rust_options).unwrap();
        assert!(
            rust_types.iter().all(|t| t.language == "rust"),
            "All should be Rust types"
        );

        // Java only
        let java_options = SearchOptions {
            language_filter: Some(vec!["java".to_string()]),
            limit: Some(100),
            ..Default::default()
        };
        let java_types = index.list_types(&java_options).unwrap();
        assert!(
            java_types.iter().all(|t| t.language == "java"),
            "All should be Java types"
        );

        // Kotlin only
        let kotlin_options = SearchOptions {
            language_filter: Some(vec!["kotlin".to_string()]),
            limit: Some(100),
            ..Default::default()
        };
        let kotlin_types = index.list_types(&kotlin_options).unwrap();
        assert!(
            kotlin_types.iter().all(|t| t.language == "kotlin"),
            "All should be Kotlin types"
        );

        // TypeScript only
        let ts_options = SearchOptions {
            language_filter: Some(vec!["typescript".to_string()]),
            limit: Some(100),
            ..Default::default()
        };
        let ts_types = index.list_types(&ts_options).unwrap();
        assert!(
            ts_types.iter().all(|t| t.language == "typescript"),
            "All should be TypeScript types"
        );
    }
}
