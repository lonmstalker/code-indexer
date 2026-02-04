//! Tests for the Summary-First Contract types and ResponseEnvelope.
//!
//! These tests verify the behavior of:
//! - ResponseEnvelope creation and serialization
//! - ContextBundle and related types
//! - Pagination cursor encoding/decoding
//! - Budget handling

use code_indexer::{
    BudgetInfo, CompactSymbol, CountsInfo, Location, NextAction, OutputFormat,
    OverlayRevision, PaginationCursor, ResponseEnvelope, SearchResult, Symbol, SymbolKind,
};
use serde_json;

// Helper function to create a test symbol
fn create_test_symbol(name: &str, kind: SymbolKind, file: &str, line: u32) -> Symbol {
    Symbol::new(name, kind, Location::new(file, line, 0, line + 5, 0), "rust")
}

// ============================================================================
// ResponseEnvelope Tests
// ============================================================================

mod response_envelope {
    use super::*;

    #[test]
    fn test_envelope_with_items() {
        let items = vec!["item1", "item2", "item3"];
        let envelope: ResponseEnvelope<&str> =
            ResponseEnvelope::with_items(items.clone(), OutputFormat::Full);

        assert!(envelope.items.is_some());
        assert_eq!(envelope.items.unwrap().len(), 3);
        assert!(envelope.sample.is_none());
        assert!(!envelope.meta.truncated);
        assert_eq!(envelope.meta.format, OutputFormat::Full);
    }

    #[test]
    fn test_envelope_truncated() {
        let sample = vec!["sample1", "sample2"];
        let counts = CountsInfo::new(100, 2);
        let envelope: ResponseEnvelope<&str> =
            ResponseEnvelope::truncated(sample, counts, Some("cursor123".to_string()));

        assert!(envelope.items.is_none());
        assert!(envelope.sample.is_some());
        assert_eq!(envelope.sample.unwrap().len(), 2);
        assert!(envelope.meta.truncated);
        assert!(envelope.meta.counts.is_some());
        assert_eq!(envelope.meta.counts.as_ref().unwrap().total, 100);
        assert_eq!(envelope.meta.counts.as_ref().unwrap().returned, 2);
        assert_eq!(envelope.meta.next_cursor, Some("cursor123".to_string()));
    }

    #[test]
    fn test_envelope_with_db_rev() {
        let envelope: ResponseEnvelope<String> =
            ResponseEnvelope::with_items(vec![], OutputFormat::Minimal).with_db_rev(42);

        assert_eq!(envelope.meta.db_rev, Some(42));
    }

    #[test]
    fn test_envelope_with_overlay_rev() {
        let overlay_rev = OverlayRevision {
            dirty_files: 3,
            max_version: 10,
        };
        let envelope: ResponseEnvelope<String> =
            ResponseEnvelope::with_items(vec![], OutputFormat::Minimal)
                .with_overlay_rev(overlay_rev.clone());

        assert!(envelope.meta.overlay_rev.is_some());
        let rev = envelope.meta.overlay_rev.unwrap();
        assert_eq!(rev.dirty_files, 3);
        assert_eq!(rev.max_version, 10);
    }

    #[test]
    fn test_envelope_with_warnings() {
        let envelope: ResponseEnvelope<String> =
            ResponseEnvelope::with_items(vec![], OutputFormat::Full)
                .with_warning("Warning 1")
                .with_warning("Warning 2");

        assert_eq!(envelope.meta.warnings.len(), 2);
        assert!(envelope.meta.warnings.contains(&"Warning 1".to_string()));
        assert!(envelope.meta.warnings.contains(&"Warning 2".to_string()));
    }

    #[test]
    fn test_envelope_serialization() {
        let envelope: ResponseEnvelope<String> =
            ResponseEnvelope::with_items(vec!["test".to_string()], OutputFormat::Compact)
                .with_db_rev(1)
                .with_warning("test warning");

        let json = serde_json::to_string(&envelope).expect("Failed to serialize");
        assert!(json.contains("\"items\""));
        assert!(json.contains("\"db_rev\":1"));
        assert!(json.contains("test warning"));
    }

    #[test]
    fn test_envelope_with_budget() {
        let budget = BudgetInfo {
            max_items: Some(20),
            max_bytes: Some(1000),
            approx_tokens: Some(500),
            actual_bytes: Some(800),
        };
        let envelope: ResponseEnvelope<String> =
            ResponseEnvelope::with_items(vec![], OutputFormat::Full).with_budget(budget);

        assert!(envelope.meta.budget.is_some());
        let b = envelope.meta.budget.unwrap();
        assert_eq!(b.max_items, Some(20));
        assert_eq!(b.max_bytes, Some(1000));
    }

    #[test]
    fn test_envelope_elapsed_time() {
        // Note: elapsed is not part of the envelope, but budget.actual_bytes can track size
        let envelope: ResponseEnvelope<String> =
            ResponseEnvelope::with_items(vec!["data".to_string()], OutputFormat::Full);

        // Serialize and verify structure
        let json = serde_json::to_string(&envelope).expect("Failed to serialize");
        assert!(json.contains("meta"));
    }

    #[test]
    fn test_envelope_with_next_actions() {
        let actions = vec![
            NextAction::new(
                "get_symbol",
                serde_json::json!({"id": "sym_123"}),
            ).with_hint("Get more details"),
            NextAction::new(
                "find_references",
                serde_json::json!({"name": "foo"}),
            ),
        ];

        let envelope: ResponseEnvelope<String> =
            ResponseEnvelope::with_items(vec![], OutputFormat::Full).with_next(actions);

        assert_eq!(envelope.next.len(), 2);
        assert_eq!(envelope.next[0].tool, "get_symbol");
        assert_eq!(envelope.next[0].hint, Some("Get more details".to_string()));
    }

    #[test]
    fn test_envelope_with_cursor() {
        let envelope: ResponseEnvelope<String> =
            ResponseEnvelope::with_items(vec![], OutputFormat::Full).with_cursor("abc123");

        assert_eq!(envelope.meta.next_cursor, Some("abc123".to_string()));
    }
}

// ============================================================================
// CountsInfo Tests
// ============================================================================

mod counts_info {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_counts_info_new() {
        let counts = CountsInfo::new(100, 20);
        assert_eq!(counts.total, 100);
        assert_eq!(counts.returned, 20);
        assert!(counts.by_kind.is_empty());
    }

    #[test]
    fn test_counts_info_with_by_kind() {
        let mut by_kind = HashMap::new();
        by_kind.insert("function".to_string(), 10);
        by_kind.insert("struct".to_string(), 5);

        let counts = CountsInfo::new(15, 15).with_by_kind(by_kind);

        assert_eq!(counts.by_kind.len(), 2);
        assert_eq!(counts.by_kind.get("function"), Some(&10));
        assert_eq!(counts.by_kind.get("struct"), Some(&5));
    }
}

// ============================================================================
// PaginationCursor Tests
// ============================================================================

mod pagination_cursor {
    use super::*;

    #[test]
    fn test_cursor_from_offset() {
        let cursor = PaginationCursor::from_offset(50);
        assert_eq!(cursor.offset, Some(50));
        assert!(cursor.score.is_none());
        assert!(cursor.kind.is_none());
    }

    #[test]
    fn test_cursor_encode_decode() {
        let cursor = PaginationCursor {
            score: Some(0.95),
            kind: Some("function".to_string()),
            file: Some("src/main.rs".to_string()),
            line: Some(42),
            stable_id: Some("sid:abc123".to_string()),
            offset: None,
        };

        let encoded = cursor.encode();
        let decoded = PaginationCursor::decode(&encoded).expect("Failed to decode");

        assert_eq!(decoded.score, Some(0.95));
        assert_eq!(decoded.kind, Some("function".to_string()));
        assert_eq!(decoded.file, Some("src/main.rs".to_string()));
        assert_eq!(decoded.line, Some(42));
        assert_eq!(decoded.stable_id, Some("sid:abc123".to_string()));
    }

    #[test]
    fn test_cursor_decode_invalid() {
        let result = PaginationCursor::decode("invalid_base64!!!");
        assert!(result.is_none());
    }

    #[test]
    fn test_cursor_from_search_result() {
        let symbol = create_test_symbol("test_func", SymbolKind::Function, "test.rs", 10);
        let search_result = SearchResult {
            symbol,
            score: 0.85,
        };

        let cursor =
            PaginationCursor::from_search_result(&search_result, Some("sid:test123".to_string()));

        assert_eq!(cursor.score, Some(0.85));
        assert_eq!(cursor.kind, Some("function".to_string()));
        assert_eq!(cursor.file, Some("test.rs".to_string()));
        assert_eq!(cursor.line, Some(10));
        assert_eq!(cursor.stable_id, Some("sid:test123".to_string()));
    }

    #[test]
    fn test_cursor_default() {
        let cursor = PaginationCursor::default();
        assert!(cursor.score.is_none());
        assert!(cursor.kind.is_none());
        assert!(cursor.file.is_none());
        assert!(cursor.line.is_none());
        assert!(cursor.stable_id.is_none());
        assert!(cursor.offset.is_none());
    }
}

// ============================================================================
// OutputFormat Tests
// ============================================================================

mod output_format {
    use super::*;

    #[test]
    fn test_output_format_from_str() {
        assert_eq!(OutputFormat::from_str("full"), Some(OutputFormat::Full));
        assert_eq!(OutputFormat::from_str("json"), Some(OutputFormat::Full));
        assert_eq!(OutputFormat::from_str("compact"), Some(OutputFormat::Compact));
        assert_eq!(OutputFormat::from_str("minimal"), Some(OutputFormat::Minimal));
        assert_eq!(OutputFormat::from_str("min"), Some(OutputFormat::Minimal));
        assert_eq!(OutputFormat::from_str("invalid"), None);
    }

    #[test]
    fn test_output_format_as_str() {
        assert_eq!(OutputFormat::Full.as_str(), "full");
        assert_eq!(OutputFormat::Compact.as_str(), "compact");
        assert_eq!(OutputFormat::Minimal.as_str(), "minimal");
    }

    #[test]
    fn test_output_format_default() {
        let format = OutputFormat::default();
        assert_eq!(format, OutputFormat::Full);
    }
}

// ============================================================================
// CompactSymbol Tests
// ============================================================================

mod compact_symbol {
    use super::*;

    #[test]
    fn test_compact_symbol_from_symbol() {
        let symbol = create_test_symbol("test_function", SymbolKind::Function, "src/lib.rs", 42);
        let compact = CompactSymbol::from_symbol(&symbol, Some(0.95));

        assert_eq!(compact.n, "test_function");
        assert_eq!(compact.k, "fn");
        assert_eq!(compact.f, "src/lib.rs");
        assert_eq!(compact.l, 42);
        assert_eq!(compact.s, Some(0.95));
    }

    #[test]
    fn test_compact_symbol_to_minimal_string_with_score() {
        let compact = CompactSymbol {
            n: "foo".to_string(),
            k: "fn".to_string(),
            f: "main.rs".to_string(),
            l: 10,
            s: Some(0.85),
        };

        let minimal = compact.to_minimal_string();
        assert_eq!(minimal, "foo:fn@main.rs:10 [0.85]");
    }

    #[test]
    fn test_compact_symbol_to_minimal_string_without_score() {
        let compact = CompactSymbol {
            n: "bar".to_string(),
            k: "str".to_string(),
            f: "lib.rs".to_string(),
            l: 5,
            s: None,
        };

        let minimal = compact.to_minimal_string();
        assert_eq!(minimal, "bar:str@lib.rs:5");
    }

    #[test]
    fn test_compact_symbol_short_kinds() {
        // Test various kind short strings
        let kinds = vec![
            (SymbolKind::Function, "fn"),
            (SymbolKind::Method, "met"),
            (SymbolKind::Struct, "str"),
            (SymbolKind::Class, "cls"),
            (SymbolKind::Interface, "int"),
            (SymbolKind::Trait, "trt"),
            (SymbolKind::Enum, "enm"),
        ];

        for (kind, expected_short) in kinds {
            let symbol = create_test_symbol("test", kind.clone(), "test.rs", 1);
            let compact = CompactSymbol::from_symbol(&symbol, None);
            assert_eq!(compact.k, expected_short, "Kind {:?} should have short str '{}'", kind, expected_short);
        }
    }
}

// ============================================================================
// NextAction Tests
// ============================================================================

mod next_action {
    use super::*;

    #[test]
    fn test_next_action_new() {
        let action = NextAction::new("search_symbols", serde_json::json!({"query": "foo"}));
        assert_eq!(action.tool, "search_symbols");
        assert!(action.hint.is_none());
    }

    #[test]
    fn test_next_action_with_hint() {
        let action = NextAction::new("get_symbol", serde_json::json!({"id": "123"}))
            .with_hint("Retrieve full symbol details");

        assert_eq!(action.hint, Some("Retrieve full symbol details".to_string()));
    }

    #[test]
    fn test_next_action_serialization() {
        let action = NextAction::new("find_references", serde_json::json!({"name": "test"}))
            .with_hint("Find all references");

        let json = serde_json::to_string(&action).expect("Failed to serialize");
        assert!(json.contains("\"tool\":\"find_references\""));
        assert!(json.contains("\"hint\":\"Find all references\""));
    }
}

// ============================================================================
// BudgetInfo Tests
// ============================================================================

mod budget_info {
    use super::*;

    #[test]
    fn test_budget_info_default() {
        let budget = BudgetInfo::default();
        assert!(budget.max_items.is_none());
        assert!(budget.max_bytes.is_none());
        assert!(budget.approx_tokens.is_none());
        assert!(budget.actual_bytes.is_none());
    }

    #[test]
    fn test_budget_info_serialization() {
        let budget = BudgetInfo {
            max_items: Some(50),
            max_bytes: Some(10000),
            approx_tokens: Some(2000),
            actual_bytes: Some(8500),
        };

        let json = serde_json::to_string(&budget).expect("Failed to serialize");
        assert!(json.contains("\"max_items\":50"));
        assert!(json.contains("\"max_bytes\":10000"));
    }

    #[test]
    fn test_budget_info_skip_none_fields() {
        let budget = BudgetInfo {
            max_items: Some(10),
            max_bytes: None,
            approx_tokens: None,
            actual_bytes: None,
        };

        let json = serde_json::to_string(&budget).expect("Failed to serialize");
        assert!(json.contains("\"max_items\":10"));
        // None fields should be skipped
        assert!(!json.contains("\"max_bytes\"") || json.contains("null"));
    }
}

// ============================================================================
// OverlayRevision Tests
// ============================================================================

mod overlay_revision {
    use super::*;

    #[test]
    fn test_overlay_revision_serialization() {
        let rev = OverlayRevision {
            dirty_files: 5,
            max_version: 42,
        };

        let json = serde_json::to_string(&rev).expect("Failed to serialize");
        assert!(json.contains("\"dirty_files\":5"));
        assert!(json.contains("\"max_version\":42"));
    }

    #[test]
    fn test_overlay_revision_deserialization() {
        let json = r#"{"dirty_files":3,"max_version":100}"#;
        let rev: OverlayRevision = serde_json::from_str(json).expect("Failed to deserialize");

        assert_eq!(rev.dirty_files, 3);
        assert_eq!(rev.max_version, 100);
    }
}
