//! Tests for MCP JSON-RPC protocol handling.
//!
//! These tests verify proper handling of JSON-RPC 2.0 protocol messages
//! including request/response formats, error handling, and edge cases.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ============================================================================
// JSON-RPC Protocol Types
// ============================================================================

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
    id: Value,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    id: Value,
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

// Standard JSON-RPC error codes
const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;

// ============================================================================
// Request Parsing Tests
// ============================================================================

mod request_parsing {
    use super::*;

    #[test]
    fn test_valid_request_structure() {
        let request_json = r#"{
            "jsonrpc": "2.0",
            "method": "search_symbols",
            "params": {"query": "test"},
            "id": 1
        }"#;

        let request: JsonRpcRequest = serde_json::from_str(request_json).expect("Failed to parse");
        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.method, "search_symbols");
        assert!(request.params.is_some());
        assert_eq!(request.id, json!(1));
    }

    #[test]
    fn test_request_without_params() {
        let request_json = r#"{
            "jsonrpc": "2.0",
            "method": "get_stats",
            "id": 2
        }"#;

        let request: JsonRpcRequest = serde_json::from_str(request_json).expect("Failed to parse");
        assert!(request.params.is_none());
    }

    #[test]
    fn test_request_with_string_id() {
        let request_json = r#"{
            "jsonrpc": "2.0",
            "method": "test",
            "id": "request-abc-123"
        }"#;

        let request: JsonRpcRequest = serde_json::from_str(request_json).expect("Failed to parse");
        assert_eq!(request.id, json!("request-abc-123"));
    }

    #[test]
    fn test_request_with_null_id() {
        let request_json = r#"{
            "jsonrpc": "2.0",
            "method": "test",
            "id": null
        }"#;

        let request: JsonRpcRequest = serde_json::from_str(request_json).expect("Failed to parse");
        assert_eq!(request.id, Value::Null);
    }

    #[test]
    fn test_invalid_json() {
        let invalid_json = r#"{ "jsonrpc": "2.0", "method": broken }"#;
        let result: Result<JsonRpcRequest, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());
    }
}

// ============================================================================
// Response Format Tests
// ============================================================================

mod response_format {
    use super::*;

    #[test]
    fn test_success_response() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(json!({"symbols": []})),
            error: None,
            id: json!(1),
        };

        let json = serde_json::to_string(&response).expect("Failed to serialize");
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_error_response() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: INVALID_PARAMS,
                message: "Invalid parameters".to_string(),
                data: None,
            }),
            id: json!(1),
        };

        let json = serde_json::to_string(&response).expect("Failed to serialize");
        assert!(json.contains("\"error\""));
        assert!(json.contains(&INVALID_PARAMS.to_string()));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn test_error_with_data() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: INTERNAL_ERROR,
                message: "Internal error".to_string(),
                data: Some(json!({"details": "Stack trace..."})),
            }),
            id: json!(1),
        };

        let json = serde_json::to_string(&response).expect("Failed to serialize");
        assert!(json.contains("\"data\""));
        assert!(json.contains("Stack trace"));
    }

    #[test]
    fn test_response_preserves_id_type() {
        // Integer ID
        let response1 = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(json!({})),
            error: None,
            id: json!(42),
        };
        let json1 = serde_json::to_string(&response1).unwrap();
        assert!(json1.contains("\"id\":42"));

        // String ID
        let response2 = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(json!({})),
            error: None,
            id: json!("test-id"),
        };
        let json2 = serde_json::to_string(&response2).unwrap();
        assert!(json2.contains("\"id\":\"test-id\""));
    }
}

// ============================================================================
// Error Code Tests
// ============================================================================

mod error_codes {
    use super::*;

    #[test]
    fn test_parse_error_code() {
        let error = JsonRpcError {
            code: PARSE_ERROR,
            message: "Parse error".to_string(),
            data: None,
        };
        assert_eq!(error.code, -32700);
    }

    #[test]
    fn test_invalid_request_code() {
        let error = JsonRpcError {
            code: INVALID_REQUEST,
            message: "Invalid Request".to_string(),
            data: None,
        };
        assert_eq!(error.code, -32600);
    }

    #[test]
    fn test_method_not_found_code() {
        let error = JsonRpcError {
            code: METHOD_NOT_FOUND,
            message: "Method not found".to_string(),
            data: None,
        };
        assert_eq!(error.code, -32601);
    }

    #[test]
    fn test_invalid_params_code() {
        let error = JsonRpcError {
            code: INVALID_PARAMS,
            message: "Invalid params".to_string(),
            data: None,
        };
        assert_eq!(error.code, -32602);
    }

    #[test]
    fn test_internal_error_code() {
        let error = JsonRpcError {
            code: INTERNAL_ERROR,
            message: "Internal error".to_string(),
            data: None,
        };
        assert_eq!(error.code, -32603);
    }
}

// ============================================================================
// MCP Tool Parameter Tests
// ============================================================================

mod tool_params {
    use super::*;

    #[test]
    fn test_search_symbols_params() {
        let params = json!({
            "query": "test",
            "limit": 10,
            "kind": "function",
            "language": "rust",
            "fuzzy": true
        });

        // Verify structure
        assert_eq!(params["query"], "test");
        assert_eq!(params["limit"], 10);
        assert_eq!(params["kind"], "function");
        assert_eq!(params["language"], "rust");
        assert_eq!(params["fuzzy"], true);
    }

    #[test]
    fn test_list_symbols_params() {
        let params = json!({
            "kind": "type",
            "language": "java",
            "file": "src/main",
            "pattern": "User*",
            "limit": 50,
            "format": "compact"
        });

        assert_eq!(params["kind"], "type");
        assert_eq!(params["format"], "compact");
    }

    #[test]
    fn test_get_symbol_params_by_id() {
        let params = json!({
            "id": "sym-123-456"
        });

        assert_eq!(params["id"], "sym-123-456");
    }

    #[test]
    fn test_get_symbol_params_by_position() {
        let params = json!({
            "file": "src/main.rs",
            "line": 42,
            "column": 10
        });

        assert_eq!(params["file"], "src/main.rs");
        assert_eq!(params["line"], 42);
        assert_eq!(params["column"], 10);
    }

    #[test]
    fn test_get_symbol_params_batch() {
        let params = json!({
            "ids": ["id1", "id2", "id3"]
        });

        let ids = params["ids"].as_array().unwrap();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn test_find_definitions_params() {
        let params = json!({
            "name": "MyClass",
            "include_deps": true,
            "dependency": "some-lib"
        });

        assert_eq!(params["name"], "MyClass");
        assert_eq!(params["include_deps"], true);
    }

    #[test]
    fn test_find_references_params() {
        let params = json!({
            "name": "process",
            "include_callers": true,
            "include_importers": false,
            "kind": "call",
            "depth": 2,
            "limit": 100
        });

        assert_eq!(params["name"], "process");
        assert_eq!(params["include_callers"], true);
        assert_eq!(params["depth"], 2);
    }

    #[test]
    fn test_analyze_call_graph_params() {
        let params = json!({
            "function": "main",
            "direction": "both",
            "depth": 5,
            "include_possible": true,
            "confidence": "all"
        });

        assert_eq!(params["function"], "main");
        assert_eq!(params["direction"], "both");
        assert_eq!(params["depth"], 5);
    }

    #[test]
    fn test_get_context_bundle_params() {
        let params = json!({
            "input": {
                "query": "test",
                "file": "src/lib.rs",
                "position": {"line": 10, "column": 5},
                "task_hint": "refactoring"
            },
            "budget": {
                "max_items": 20,
                "max_bytes": 10000,
                "snippet_lines": 3
            },
            "format": "minimal"
        });

        assert_eq!(params["input"]["query"], "test");
        assert_eq!(params["budget"]["max_items"], 20);
        assert_eq!(params["format"], "minimal");
    }

    #[test]
    fn test_update_files_params() {
        let params = json!({
            "files": [
                {
                    "path": "src/main.rs",
                    "content": "fn main() {}",
                    "version": 1
                },
                {
                    "path": "src/lib.rs",
                    "content": "pub mod utils;"
                }
            ]
        });

        let files = params["files"].as_array().unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0]["path"], "src/main.rs");
    }
}

// ============================================================================
// Batch Request Tests
// ============================================================================

mod batch_requests {
    use super::*;

    #[test]
    fn test_batch_request_array() {
        let batch_json = r#"[
            {"jsonrpc": "2.0", "method": "get_stats", "id": 1},
            {"jsonrpc": "2.0", "method": "list_symbols", "params": {"kind": "function"}, "id": 2}
        ]"#;

        let batch: Vec<JsonRpcRequest> = serde_json::from_str(batch_json).expect("Failed to parse");
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].method, "get_stats");
        assert_eq!(batch[1].method, "list_symbols");
    }

    #[test]
    fn test_batch_response_array() {
        let responses = vec![
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(json!({"total_symbols": 100})),
                error: None,
                id: json!(1),
            },
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(json!({"symbols": []})),
                error: None,
                id: json!(2),
            },
        ];

        let json = serde_json::to_string(&responses).expect("Failed to serialize");
        assert!(json.starts_with('['));
        assert!(json.ends_with(']'));
    }

    #[test]
    fn test_mixed_batch_response() {
        let responses = vec![
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(json!({})),
                error: None,
                id: json!(1),
            },
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(JsonRpcError {
                    code: INVALID_PARAMS,
                    message: "Bad params".to_string(),
                    data: None,
                }),
                id: json!(2),
            },
        ];

        let json = serde_json::to_string(&responses).expect("Failed to serialize");
        assert!(json.contains("\"result\""));
        assert!(json.contains("\"error\""));
    }
}

// ============================================================================
// Notification Tests (no id field)
// ============================================================================

mod notifications {
    use super::*;

    /// JSON-RPC 2.0 Notification (no id)
    #[derive(Debug, Serialize, Deserialize)]
    struct JsonRpcNotification {
        jsonrpc: String,
        method: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<Value>,
    }

    #[test]
    fn test_notification_structure() {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "file_changed".to_string(),
            params: Some(json!({"path": "src/main.rs"})),
        };

        let json = serde_json::to_string(&notification).expect("Failed to serialize");
        assert!(!json.contains("\"id\""));
        assert!(json.contains("\"method\":\"file_changed\""));
    }

    #[test]
    fn test_notification_without_params() {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "index_complete".to_string(),
            params: None,
        };

        let json = serde_json::to_string(&notification).expect("Failed to serialize");
        assert!(!json.contains("\"params\""));
    }
}

// ============================================================================
// Edge Cases
// ============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn test_empty_params_object() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: Some(json!({})),
            id: json!(1),
        };

        let json = serde_json::to_string(&request).expect("Failed to serialize");
        assert!(json.contains("\"params\":{}"));
    }

    #[test]
    fn test_params_with_special_characters() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "search_symbols".to_string(),
            params: Some(json!({"query": "test\nwith\ttabs"})),
            id: json!(1),
        };

        let json = serde_json::to_string(&request).expect("Failed to serialize");
        // Newlines should be escaped
        assert!(json.contains("\\n") || json.contains("\\t"));
    }

    #[test]
    fn test_large_id_number() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: None,
            id: json!(9007199254740991i64), // Max safe integer
        };

        let json = serde_json::to_string(&request).expect("Failed to serialize");
        assert!(json.contains("9007199254740991"));
    }

    #[test]
    fn test_unicode_in_params() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "search_symbols".to_string(),
            params: Some(json!({"query": "Привет мир 日本語"})),
            id: json!(1),
        };

        let json = serde_json::to_string(&request).expect("Failed to serialize");
        let parsed: JsonRpcRequest = serde_json::from_str(&json).expect("Failed to parse");
        assert_eq!(
            parsed.params.unwrap()["query"],
            "Привет мир 日本語"
        );
    }

    #[test]
    fn test_deeply_nested_params() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: Some(json!({
                "level1": {
                    "level2": {
                        "level3": {
                            "level4": {
                                "value": "deep"
                            }
                        }
                    }
                }
            })),
            id: json!(1),
        };

        let json = serde_json::to_string(&request).expect("Failed to serialize");
        let parsed: JsonRpcRequest = serde_json::from_str(&json).expect("Failed to parse");
        assert_eq!(
            parsed.params.unwrap()["level1"]["level2"]["level3"]["level4"]["value"],
            "deep"
        );
    }
}
