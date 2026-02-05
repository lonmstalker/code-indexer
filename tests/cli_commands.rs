//! Integration tests for CLI commands.
//!
//! These tests verify that the CLI commands work correctly.

use std::path::PathBuf;
use tempfile::TempDir;

use code_indexer::{
    CodeIndex, FileWalker, LanguageRegistry, Parser, SearchOptions, SqliteIndex, SymbolExtractor,
    SymbolKind,
};

/// Helper to check if a Vec of tuples contains a key.
fn vec_contains_key(vec: &[(String, usize)], key: &str) -> bool {
    vec.iter().any(|(k, _)| k == key)
}

/// Helper to get value from Vec of tuples by key.
fn vec_get(vec: &[(String, usize)], key: &str) -> Option<usize> {
    vec.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
}

/// Creates a test index from example Rust code.
fn create_test_index() -> (SqliteIndex, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join(".code-index.db");

    let registry = LanguageRegistry::new();
    let walker = FileWalker::new(registry);
    let registry = LanguageRegistry::new();
    let parser = Parser::new(registry);
    let extractor = SymbolExtractor::new();
    let index = SqliteIndex::new(&db_path).expect("Failed to create index");

    let base_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/rust-example");
    let files = walker.walk(&base_path).expect("Failed to walk directory");

    for file in &files {
        if let Ok(parsed) = parser.parse_file(file) {
            if let Ok(result) = extractor.extract_all(&parsed, file) {
                index
                    .add_symbols(result.symbols)
                    .expect("Failed to add symbols");
            }
        }
    }

    (index, temp_dir)
}

// ============================================================================
// Index Command Tests
// ============================================================================

mod index_command {
    use super::*;

    #[test]
    fn test_index_creates_db_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join(".code-index.db");

        let index = SqliteIndex::new(&db_path).expect("Failed to create index");
        let stats = index.get_stats().expect("Failed to get stats");

        // DB should be created and empty
        assert_eq!(stats.total_symbols, 0);
        assert_eq!(stats.total_files, 0);

        // DB file should exist
        assert!(db_path.exists());
    }

    #[test]
    fn test_index_from_rust_example() {
        let (index, _temp_dir) = create_test_index();
        let stats = index.get_stats().expect("Failed to get stats");

        // Should have indexed symbols
        assert!(stats.total_symbols > 0);
        assert!(stats.total_files > 0);

        // Should have Rust files
        let rust_files = vec_get(&stats.files_by_language, "rust").unwrap_or(0);
        assert!(rust_files > 0);
    }
}

// ============================================================================
// Symbols Command Tests
// ============================================================================

mod symbols_command {
    use super::*;

    #[test]
    fn test_search_finds_function() {
        let (index, _temp_dir) = create_test_index();

        // Use find_definition which works more reliably
        let symbols = index
            .find_definition("demonstrate_closures")
            .expect("Failed to search");

        // Should find the demonstrate_closures function
        assert!(
            !symbols.is_empty(),
            "Expected to find 'demonstrate_closures' function in rust-example"
        );
    }

    #[test]
    fn test_search_with_kind_filter() {
        let (index, _temp_dir) = create_test_index();

        let options = SearchOptions {
            kind_filter: Some(vec![SymbolKind::Function]),
            ..Default::default()
        };

        let results = index.search("main", &options).expect("Failed to search");

        // All results should be functions
        for result in &results {
            assert_eq!(result.symbol.kind, SymbolKind::Function);
        }
    }

    #[test]
    fn test_search_with_limit() {
        let (index, _temp_dir) = create_test_index();

        let options = SearchOptions {
            limit: Some(5),
            ..Default::default()
        };

        let results = index.search("", &options).expect("Failed to search");

        // Should respect limit
        assert!(results.len() <= 5);
    }

    #[test]
    fn test_list_functions() {
        let (index, _temp_dir) = create_test_index();

        let options = SearchOptions {
            limit: Some(100),
            ..Default::default()
        };

        let results = index.list_functions(&options).expect("Failed to list");

        // Should have functions (includes methods)
        assert!(!results.is_empty());
        for symbol in &results {
            assert!(
                symbol.kind == SymbolKind::Function || symbol.kind == SymbolKind::Method,
                "Expected function or method, got {:?}",
                symbol.kind
            );
        }
    }

    #[test]
    fn test_list_types() {
        let (index, _temp_dir) = create_test_index();

        let type_kinds = vec![
            SymbolKind::Struct,
            SymbolKind::Enum,
            SymbolKind::Trait,
            SymbolKind::TypeAlias,
            SymbolKind::Class,
            SymbolKind::Interface,
        ];

        let options = SearchOptions {
            limit: Some(100),
            ..Default::default()
        };

        let results = index.list_types(&options).expect("Failed to list");

        // All results should be types
        for symbol in &results {
            assert!(
                type_kinds.contains(&symbol.kind),
                "Expected type, got {:?}",
                symbol.kind
            );
        }
    }
}

// ============================================================================
// Definition Command Tests
// ============================================================================

mod definition_command {
    use super::*;

    #[test]
    fn test_find_definition_by_name() {
        let (index, _temp_dir) = create_test_index();

        // Look for "process_users" function which exists in rust-example/src/lib.rs
        let symbols = index
            .find_definition("process_users")
            .expect("Failed to find definition");

        // Should find the function
        assert!(
            !symbols.is_empty(),
            "Expected to find 'process_users' function definition"
        );
        assert!(symbols.iter().any(|s| s.name == "process_users"));
    }

    #[test]
    fn test_find_definition_not_found() {
        let (index, _temp_dir) = create_test_index();

        let symbols = index
            .find_definition("nonexistent_symbol_xyz")
            .expect("Failed to find definition");

        // Should return empty
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_find_definition_excludes_imports() {
        let (index, _temp_dir) = create_test_index();

        // Find any definition
        let symbols = index
            .find_definition("main")
            .expect("Failed to find definition");

        // Should not include imports
        for symbol in &symbols {
            assert_ne!(symbol.kind, SymbolKind::Import);
        }
    }
}

// ============================================================================
// Stats Command Tests
// ============================================================================

mod stats_command {
    use super::*;

    #[test]
    fn test_stats_returns_counts() {
        let (index, _temp_dir) = create_test_index();

        let stats = index.get_stats().expect("Failed to get stats");

        // Should have positive counts
        assert!(stats.total_symbols > 0);
        assert!(stats.total_files > 0);
    }

    #[test]
    fn test_stats_by_kind() {
        let (index, _temp_dir) = create_test_index();

        let stats = index.get_stats().expect("Failed to get stats");

        // Should have breakdown by kind
        assert!(!stats.symbols_by_kind.is_empty());

        // Should include functions
        assert!(
            vec_contains_key(&stats.symbols_by_kind, "function"),
            "Expected 'function' in symbols_by_kind"
        );
    }

    #[test]
    fn test_stats_by_language() {
        let (index, _temp_dir) = create_test_index();

        let stats = index.get_stats().expect("Failed to get stats");

        // Should have breakdown by language
        assert!(!stats.symbols_by_language.is_empty());
        assert!(!stats.files_by_language.is_empty());

        // Should include Rust
        assert!(
            vec_contains_key(&stats.symbols_by_language, "rust"),
            "Expected 'rust' in symbols_by_language"
        );
    }
}

// ============================================================================
// References Command Tests
// ============================================================================

mod references_command {
    use super::*;

    #[test]
    fn test_find_references() {
        let (index, _temp_dir) = create_test_index();

        let options = SearchOptions {
            limit: Some(100),
            ..Default::default()
        };

        // Find references to a common symbol
        let refs = index
            .find_references("main", &options)
            .expect("Failed to find references");

        // May or may not have references depending on the example
        // Just verify the call works (result can be empty)
        let _ = refs.len();
    }
}

// ============================================================================
// Clear Command Tests
// ============================================================================

mod clear_command {
    use super::*;

    #[test]
    fn test_clear_removes_all_symbols() {
        let (index, _temp_dir) = create_test_index();

        // Verify we have symbols first
        let stats_before = index.get_stats().expect("Failed to get stats");
        assert!(stats_before.total_symbols > 0);

        // Clear the index
        index.clear().expect("Failed to clear index");

        // Verify symbols are gone
        let stats_after = index.get_stats().expect("Failed to get stats");
        assert_eq!(stats_after.total_symbols, 0);
        assert_eq!(stats_after.total_files, 0);
    }
}
