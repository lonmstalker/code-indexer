//! Integration tests for File Tags and Intent Layer functionality.
//!
//! Tests the full flow from sidecar parsing to database storage to MCP API queries.

use code_indexer::index::{FileMeta, FileTag, MetaSource, Stability, TagDictionary};
use code_indexer::index::sqlite::SqliteIndex;
use code_indexer::indexer::{
    check_staleness, compute_exported_hash, extract_file_meta, extract_file_tags,
    parse_sidecar, resolve_tags,
};

/// Helper to create an in-memory index with standard schema
fn create_test_index() -> SqliteIndex {
    SqliteIndex::in_memory().expect("Failed to create in-memory index")
}

// =====================================================
// Sidecar Parsing Tests
// =====================================================

#[test]
fn test_sidecar_full_example() {
    let content = r#"
directory_tags:
  - domain:auth
  - layer:service

files:
  service.rs:
    doc1: "Authentication service with JWT and OAuth2"
    purpose: "Centralizes token generation and validation logic"
    capabilities:
      - jwt_generation
      - oauth2_flow
      - token_refresh
    invariants:
      - "refresh_token is stored as hash only"
      - "access_token max lifetime is 15 minutes"
    non_goals:
      - "does not handle authorization (RBAC)"
      - "does not store sessions"
    security_notes: "all token operations are logged"
    owner: "team-security"
    stability: stable
    tags:
      - pattern:idempotency

  token.rs:
    doc1: "JWT token utilities"
    stability: experimental
"#;

    let data = parse_sidecar(content).unwrap();

    // Directory tags
    assert_eq!(data.directory_tags.len(), 2);
    assert!(data.directory_tags.contains(&"domain:auth".to_string()));

    // Service file
    let service = data.files.get("service.rs").unwrap();
    assert_eq!(
        service.doc1,
        Some("Authentication service with JWT and OAuth2".to_string())
    );
    assert_eq!(service.capabilities.len(), 3);
    assert_eq!(service.invariants.len(), 2);
    assert_eq!(service.non_goals.len(), 2);
    assert_eq!(service.owner, Some("team-security".to_string()));
    assert_eq!(service.stability, Some("stable".to_string()));

    // Token file
    let token = data.files.get("token.rs").unwrap();
    assert_eq!(token.doc1, Some("JWT token utilities".to_string()));
    assert_eq!(token.stability, Some("experimental".to_string()));
}

#[test]
fn test_extract_meta_and_tags_from_sidecar() {
    let content = r#"
directory_tags:
  - domain:auth

files:
  handler.rs:
    doc1: "Request handler"
    stability: stable
    tags:
      - layer:api
"#;

    let data = parse_sidecar(content).unwrap();

    // Extract file meta
    let meta = extract_file_meta("src/auth/handler.rs", &data, "src/auth").unwrap();
    assert_eq!(meta.file_path, "src/auth/handler.rs");
    assert_eq!(meta.doc1, Some("Request handler".to_string()));
    assert_eq!(meta.stability, Some(Stability::Stable));
    assert_eq!(meta.source, MetaSource::Sidecar);
    assert_eq!(meta.confidence, 1.0);

    // Extract tags (directory + file-specific)
    let tags = extract_file_tags("src/auth/handler.rs", &data);
    assert_eq!(tags.len(), 2);
    assert!(tags.contains(&"domain:auth".to_string()));
    assert!(tags.contains(&"layer:api".to_string()));
}

// =====================================================
// Database CRUD Tests
// =====================================================

#[test]
fn test_file_meta_crud() {
    let index = create_test_index();

    // Create
    let meta = FileMeta::new("src/auth/service.rs")
        .with_doc1("Auth service")
        .with_purpose("Token handling")
        .with_capabilities(vec!["jwt".to_string(), "oauth".to_string()])
        .with_stability(Stability::Stable)
        .with_source(MetaSource::Sidecar)
        .with_owner("team-security")
        .with_exported_hash("abc123");

    index.upsert_file_meta(&meta).unwrap();

    // Read
    let retrieved = index.get_file_meta("src/auth/service.rs").unwrap().unwrap();
    assert_eq!(retrieved.doc1, Some("Auth service".to_string()));
    assert_eq!(retrieved.purpose, Some("Token handling".to_string()));
    assert_eq!(retrieved.capabilities.len(), 2);
    assert_eq!(retrieved.stability, Some(Stability::Stable));
    assert_eq!(retrieved.owner, Some("team-security".to_string()));
    assert_eq!(retrieved.exported_hash, Some("abc123".to_string()));

    // Update
    let updated = FileMeta::new("src/auth/service.rs")
        .with_doc1("Updated auth service")
        .with_stability(Stability::Deprecated);
    index.upsert_file_meta(&updated).unwrap();

    let retrieved = index.get_file_meta("src/auth/service.rs").unwrap().unwrap();
    assert_eq!(retrieved.doc1, Some("Updated auth service".to_string()));
    assert_eq!(retrieved.stability, Some(Stability::Deprecated));

    // Delete
    index.delete_file_meta("src/auth/service.rs").unwrap();
    assert!(index.get_file_meta("src/auth/service.rs").unwrap().is_none());
}

#[test]
fn test_tag_dictionary_with_synonyms() {
    let index = create_test_index();

    // Get seed tags
    let tags = index.get_tag_dictionary().unwrap();
    assert!(!tags.is_empty());

    // Find auth tag
    let auth_tag = tags.iter().find(|t| t.canonical_name == "auth").unwrap();
    assert_eq!(auth_tag.category, "domain");

    // Test synonym resolution
    let resolved = index.resolve_tag_synonym("authn").unwrap();
    assert!(resolved.is_some());
    assert_eq!(resolved.unwrap().canonical_name, "auth");

    // Test synonym "login"
    let resolved = index.resolve_tag_synonym("login").unwrap();
    assert!(resolved.is_some());
    assert_eq!(resolved.unwrap().canonical_name, "auth");
}

#[test]
fn test_file_tags_with_dictionary() {
    let index = create_test_index();

    // Get tag IDs from dictionary
    let auth_tag = index.get_tag_by_name("auth").unwrap().unwrap();
    let service_tag = index.get_tag_by_name("service").unwrap().unwrap();

    // Add tags to file
    let file_tags = vec![
        FileTag::new("src/auth/service.rs", auth_tag.id)
            .with_source(MetaSource::Sidecar)
            .with_confidence(1.0),
        FileTag::new("src/auth/service.rs", service_tag.id)
            .with_source(MetaSource::Sidecar)
            .with_confidence(1.0),
    ];

    index.add_file_tags("src/auth/service.rs", &file_tags).unwrap();

    // Retrieve and verify
    let retrieved = index.get_file_tags("src/auth/service.rs").unwrap();
    assert_eq!(retrieved.len(), 2);

    // Tags should have names and categories filled in
    assert!(retrieved.iter().any(|t| t.tag_name == Some("auth".to_string())));
    assert!(retrieved.iter().any(|t| t.tag_name == Some("service".to_string())));
}

#[test]
fn test_search_by_tags() {
    let index = create_test_index();

    let auth_tag = index.get_tag_by_name("auth").unwrap().unwrap();
    let api_tag = index.get_tag_by_name("api").unwrap().unwrap();
    let service_tag = index.get_tag_by_name("service").unwrap().unwrap();

    // File 1: auth + service
    index.add_file_tags("src/auth/service.rs", &[
        FileTag::new("src/auth/service.rs", auth_tag.id).with_source(MetaSource::Sidecar),
        FileTag::new("src/auth/service.rs", service_tag.id).with_source(MetaSource::Sidecar),
    ]).unwrap();

    // File 2: auth + api
    index.add_file_tags("src/auth/handler.rs", &[
        FileTag::new("src/auth/handler.rs", auth_tag.id).with_source(MetaSource::Sidecar),
        FileTag::new("src/auth/handler.rs", api_tag.id).with_source(MetaSource::Sidecar),
    ]).unwrap();

    // File 3: service only
    index.add_file_tags("src/payment/processor.rs", &[
        FileTag::new("src/payment/processor.rs", service_tag.id).with_source(MetaSource::Sidecar),
    ]).unwrap();

    // Search: auth (should match 2 files)
    let files = index.search_files_by_tags(&["auth".to_string()]).unwrap();
    assert_eq!(files.len(), 2);

    // Search: domain:auth (with category)
    let files = index.search_files_by_tags(&["domain:auth".to_string()]).unwrap();
    assert_eq!(files.len(), 2);

    // Search: auth AND service (should match 1 file)
    let files = index.search_files_by_tags(&["auth".to_string(), "service".to_string()]).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0], "src/auth/service.rs");

    // Search: service (should match 2 files)
    let files = index.search_files_by_tags(&["service".to_string()]).unwrap();
    assert_eq!(files.len(), 2);
}

#[test]
fn test_file_meta_fts_search() {
    let index = create_test_index();

    // Add file metadata
    index.upsert_file_meta(&FileMeta::new("src/auth/jwt.rs")
        .with_doc1("JWT token generation and validation")
        .with_purpose("Handle JSON Web Tokens securely")).unwrap();

    index.upsert_file_meta(&FileMeta::new("src/auth/oauth.rs")
        .with_doc1("OAuth2 flow implementation")
        .with_purpose("Handle OAuth2 authorization code flow")).unwrap();

    index.upsert_file_meta(&FileMeta::new("src/payments/stripe.rs")
        .with_doc1("Stripe payment processing")
        .with_purpose("Process credit card payments")).unwrap();

    // FTS search for "JWT"
    let results = index.search_file_meta("JWT", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file_path, "src/auth/jwt.rs");

    // FTS search for "payment"
    let results = index.search_file_meta("payment", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file_path, "src/payments/stripe.rs");

    // FTS search for "OAuth2"
    let results = index.search_file_meta("OAuth2", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file_path, "src/auth/oauth.rs");
}

// =====================================================
// Staleness Detection Tests
// =====================================================

#[test]
fn test_staleness_detection_workflow() {
    use code_indexer::index::{Location, Symbol, SymbolKind, Visibility};

    let index = create_test_index();

    // Initial symbols
    let symbols_v1 = vec![
        Symbol::new("authenticate", SymbolKind::Function, Location::new("auth.rs", 1, 0, 10, 1), "rust")
            .with_visibility(Visibility::Public)
            .with_signature("fn authenticate(user: &str) -> bool"),
    ];

    // Store initial hash
    let hash_v1 = compute_exported_hash(&symbols_v1);
    let meta = FileMeta::new("src/auth.rs")
        .with_doc1("Auth module")
        .with_exported_hash(&hash_v1);
    index.upsert_file_meta(&meta).unwrap();

    // Check staleness - should be fresh
    let (is_stale, _) = check_staleness(&symbols_v1, Some(&hash_v1));
    assert!(!is_stale);

    // Modify symbols (add parameter)
    let symbols_v2 = vec![
        Symbol::new("authenticate", SymbolKind::Function, Location::new("auth.rs", 1, 0, 10, 1), "rust")
            .with_visibility(Visibility::Public)
            .with_signature("fn authenticate(user: &str, password: &str) -> bool"), // Changed!
    ];

    // Check staleness - should be stale now
    let (is_stale, new_hash) = check_staleness(&symbols_v2, Some(&hash_v1));
    assert!(is_stale);
    assert_ne!(new_hash, hash_v1);
}

#[test]
fn test_tag_resolution_flow() {
    let index = create_test_index();

    // Get tag dictionary
    let dict = index.get_tag_dictionary().unwrap();

    // Resolve tags from sidecar
    let tag_strings = vec![
        "domain:auth".to_string(),
        "layer:service".to_string(),
        "authn".to_string(), // synonym for auth
    ];

    let resolved = resolve_tags("src/auth.rs", &tag_strings, &dict);

    // Should have resolved auth twice (once direct, once via synonym) + service
    // But auth should dedupe on insertion due to UNIQUE constraint
    assert!(resolved.iter().any(|t| t.tag_name == Some("auth".to_string())));
    assert!(resolved.iter().any(|t| t.tag_name == Some("service".to_string())));
}

// =====================================================
// Combined Meta + Tags Tests
// =====================================================

#[test]
fn test_get_file_meta_with_tags() {
    let index = create_test_index();

    // Add file meta
    index.upsert_file_meta(&FileMeta::new("src/auth/service.rs")
        .with_doc1("Authentication service")
        .with_stability(Stability::Stable)
        .with_source(MetaSource::Sidecar)).unwrap();

    // Add tags
    let auth_tag = index.get_tag_by_name("auth").unwrap().unwrap();
    let service_tag = index.get_tag_by_name("service").unwrap().unwrap();
    index.add_file_tags("src/auth/service.rs", &[
        FileTag::new("src/auth/service.rs", auth_tag.id).with_source(MetaSource::Sidecar),
        FileTag::new("src/auth/service.rs", service_tag.id).with_source(MetaSource::Sidecar),
    ]).unwrap();

    // Get combined
    let (meta, tags) = index.get_file_meta_with_tags("src/auth/service.rs").unwrap().unwrap();

    assert_eq!(meta.doc1, Some("Authentication service".to_string()));
    assert_eq!(meta.stability, Some(Stability::Stable));
    assert_eq!(tags.len(), 2);

    // Tags should have full info
    let tag_names: Vec<_> = tags.iter().filter_map(|t| t.tag_name.clone()).collect();
    assert!(tag_names.contains(&"auth".to_string()));
    assert!(tag_names.contains(&"service".to_string()));
}

#[test]
fn test_tag_stats() {
    let index = create_test_index();

    let auth_tag = index.get_tag_by_name("auth").unwrap().unwrap();
    let service_tag = index.get_tag_by_name("service").unwrap().unwrap();

    // Add tags to multiple files
    for i in 0..3 {
        let path = format!("src/auth/file{}.rs", i);
        index.add_file_tags(&path, &[
            FileTag::new(&path, auth_tag.id).with_source(MetaSource::Sidecar),
        ]).unwrap();
    }

    index.add_file_tags("src/service.rs", &[
        FileTag::new("src/service.rs", service_tag.id).with_source(MetaSource::Sidecar),
    ]).unwrap();

    let stats = index.get_tag_stats().unwrap();

    // auth should have 3 files
    let auth_stat = stats.iter().find(|(_, name, _)| name == "auth");
    assert!(auth_stat.is_some());
    assert_eq!(auth_stat.unwrap().2, 3);

    // service should have 1 file
    let service_stat = stats.iter().find(|(_, name, _)| name == "service");
    assert!(service_stat.is_some());
    assert_eq!(service_stat.unwrap().2, 1);
}

#[test]
fn test_custom_tag_upsert() {
    let index = create_test_index();

    // Add custom tag
    let new_tag = TagDictionary::new("websocket", "pattern")
        .with_display_name("WebSocket")
        .with_synonyms(vec!["ws".to_string(), "realtime".to_string()]);

    let tag_id = index.upsert_tag(&new_tag).unwrap();
    assert!(tag_id > 0);

    // Should be findable
    let retrieved = index.get_tag_by_name("websocket").unwrap().unwrap();
    assert_eq!(retrieved.category, "pattern");
    assert_eq!(retrieved.display_name, Some("WebSocket".to_string()));

    // Synonyms should resolve
    let resolved = index.resolve_tag_synonym("ws").unwrap();
    assert!(resolved.is_some());
    assert_eq!(resolved.unwrap().canonical_name, "websocket");
}
