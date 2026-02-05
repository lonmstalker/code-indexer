//! Parser for .code-indexer.yml sidecar files.
//!
//! Sidecar files provide file-level metadata (Intent Layer) including:
//! - doc1: one-line summary
//! - purpose: what the file does
//! - capabilities/invariants/non_goals: semantic information
//! - tags: categorization for search and navigation
//!
//! Format:
//! ```yaml
//! directory_tags:
//!   - domain:auth
//!   - layer:service
//!
//! files:
//!   service.rs:
//!     doc1: "Authentication service with JWT and OAuth2"
//!     purpose: "Centralizes token generation and validation"
//!     capabilities:
//!       - jwt_generation
//!       - oauth2_flow
//!     tags:
//!       - pattern:idempotency
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::error::Result;
use crate::index::{FileMeta, FileTag, MetaSource, Stability, TagDictionary};

/// Name of the sidecar file
pub const SIDECAR_FILENAME: &str = ".code-indexer.yml";

/// Parsed sidecar file data
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SidecarData {
    /// Tags that apply to all files in the directory
    #[serde(default)]
    pub directory_tags: Vec<String>,

    /// Per-file metadata
    #[serde(default)]
    pub files: HashMap<String, FileMetadata>,
}

/// Per-file metadata from sidecar
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileMetadata {
    /// One-line summary (~50 chars)
    pub doc1: Option<String>,

    /// Purpose description
    pub purpose: Option<String>,

    /// Capabilities provided by this file
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// Invariants that must be maintained
    #[serde(default)]
    pub invariants: Vec<String>,

    /// Non-goals: what this file explicitly does NOT do
    #[serde(default)]
    pub non_goals: Vec<String>,

    /// Security-related notes
    pub security_notes: Option<String>,

    /// Owner (team or person)
    pub owner: Option<String>,

    /// Stability level
    pub stability: Option<String>,

    /// File-specific tags (added to directory_tags)
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Parses a sidecar file content
pub fn parse_sidecar(content: &str) -> Result<SidecarData> {
    let data: SidecarData = serde_yaml::from_str(content)
        .map_err(|e| crate::error::IndexerError::Parse(format!("Invalid sidecar YAML: {}", e)))?;
    Ok(data)
}

/// Extracts FileMeta for a specific file from sidecar data
pub fn extract_file_meta(
    file_path: &str,
    sidecar: &SidecarData,
    _dir_path: &str,
) -> Option<FileMeta> {
    let filename = Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())?;

    let file_meta = sidecar.files.get(filename)?;

    let stability = file_meta
        .stability
        .as_ref()
        .and_then(|s| Stability::from_str(s));

    Some(FileMeta {
        file_path: file_path.to_string(),
        doc1: file_meta.doc1.clone(),
        purpose: file_meta.purpose.clone(),
        capabilities: file_meta.capabilities.clone(),
        invariants: file_meta.invariants.clone(),
        non_goals: file_meta.non_goals.clone(),
        security_notes: file_meta.security_notes.clone(),
        owner: file_meta.owner.clone(),
        stability,
        exported_hash: None,
        last_extracted: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        source: MetaSource::Sidecar,
        confidence: 1.0,
        is_stale: false,
    })
}

/// Extracts all tags for a file from sidecar data (directory + file-specific)
pub fn extract_file_tags(
    file_path: &str,
    sidecar: &SidecarData,
) -> Vec<String> {
    let filename = Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let mut tags = sidecar.directory_tags.clone();

    if let Some(file_meta) = sidecar.files.get(filename) {
        tags.extend(file_meta.tags.clone());
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    tags.retain(|t| seen.insert(t.clone()));

    tags
}

/// Parses a tag string into (category, name) tuple
/// Supports formats: "category:name" or just "name"
pub fn parse_tag(tag: &str) -> (Option<&str>, &str) {
    if let Some(pos) = tag.find(':') {
        (Some(&tag[..pos]), &tag[pos + 1..])
    } else {
        (None, tag)
    }
}

/// Resolves tag strings to FileTag structs using the tag dictionary
pub fn resolve_tags(
    file_path: &str,
    tag_strings: &[String],
    tag_dict: &[TagDictionary],
) -> Vec<FileTag> {
    let mut resolved = Vec::new();

    for tag_str in tag_strings {
        let (category, name) = parse_tag(tag_str);

        // Find matching tag in dictionary
        let dict_entry = tag_dict.iter().find(|t| {
            let name_matches = t.canonical_name == name || t.matches(name);
            let category_matches = category.map(|c| t.category == c).unwrap_or(true);
            name_matches && category_matches
        });

        if let Some(entry) = dict_entry {
            resolved.push(
                FileTag::new(file_path, entry.id)
                    .with_source(MetaSource::Sidecar)
                    .with_confidence(1.0)
                    .with_tag_name(&entry.canonical_name)
                    .with_tag_category(&entry.category),
            );
        }
        // Unknown tags are silently ignored (could add warning in future)
    }

    resolved
}

/// Finds the sidecar file path for a given source file
pub fn find_sidecar_path(source_file: &str) -> Option<String> {
    let path = Path::new(source_file);
    let dir = path.parent()?;
    let sidecar = dir.join(SIDECAR_FILENAME);
    Some(sidecar.to_string_lossy().to_string())
}

/// Extracts front-matter from source file comments
/// Looks for @code-indexer marker in file header comments
pub fn extract_front_matter(content: &str, language: &str) -> Option<FileMetadata> {
    let comment_prefix = match language {
        "rust" => "//!",
        "python" => "#",
        "javascript" | "typescript" => "//",
        "go" => "//",
        "java" | "kotlin" => "//",
        "c" | "cpp" => "//",
        _ => return None,
    };

    let mut in_front_matter = false;
    let mut yaml_lines = Vec::new();

    for line in content.lines().take(50) {
        // Limit to first 50 lines
        let trimmed = line.trim();

        if !trimmed.starts_with(comment_prefix) && !trimmed.is_empty() {
            // Non-comment, non-empty line - stop parsing
            break;
        }

        let comment_content = trimmed
            .strip_prefix(comment_prefix)
            .map(|s| s.trim())
            .unwrap_or("");

        if comment_content == "@code-indexer" {
            in_front_matter = true;
            continue;
        }

        if in_front_matter {
            if comment_content.is_empty() || !comment_content.contains(':') {
                // Empty line or no YAML-like content - end of front-matter
                break;
            }
            yaml_lines.push(comment_content.to_string());
        }
    }

    if yaml_lines.is_empty() {
        return None;
    }

    // Parse as YAML
    let yaml_content = yaml_lines.join("\n");
    serde_yaml::from_str(&yaml_content).ok()
}

/// Computes a hash of exported (public) symbols for staleness detection.
///
/// The hash is based on public symbols' names, kinds, and normalized signatures.
/// When this hash differs from the stored `exported_hash` in file_meta,
/// the documentation is considered stale.
pub fn compute_exported_hash(symbols: &[crate::index::Symbol]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Filter to public symbols and sort for determinism
    let mut exported: Vec<_> = symbols
        .iter()
        .filter(|s| {
            matches!(
                s.visibility,
                Some(crate::index::Visibility::Public) | Some(crate::index::Visibility::Internal)
            )
        })
        .collect();

    exported.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.kind.as_str().cmp(b.kind.as_str()))
    });

    let mut hasher = DefaultHasher::new();

    for symbol in exported {
        symbol.name.hash(&mut hasher);
        symbol.kind.as_str().hash(&mut hasher);

        // Normalize signature (remove extra whitespace)
        if let Some(ref sig) = symbol.signature {
            let normalized: String = sig.split_whitespace().collect::<Vec<_>>().join(" ");
            normalized.hash(&mut hasher);
        }
    }

    format!("{:016x}", hasher.finish())
}

/// Checks if a file's documentation is stale by comparing exported hashes.
///
/// Returns a tuple of (is_stale, current_hash).
pub fn check_staleness(
    symbols: &[crate::index::Symbol],
    stored_hash: Option<&str>,
) -> (bool, String) {
    let current_hash = compute_exported_hash(symbols);

    let is_stale = match stored_hash {
        Some(stored) => stored != current_hash,
        None => false, // No stored hash means no staleness check possible
    };

    (is_stale, current_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sidecar_basic() {
        let content = r#"
directory_tags:
  - domain:auth
  - layer:service

files:
  service.rs:
    doc1: "Authentication service"
    purpose: "Handles JWT tokens"
    capabilities:
      - jwt_generation
    tags:
      - pattern:idempotency
"#;

        let data = parse_sidecar(content).unwrap();

        assert_eq!(data.directory_tags.len(), 2);
        assert_eq!(data.directory_tags[0], "domain:auth");

        let service = data.files.get("service.rs").unwrap();
        assert_eq!(service.doc1, Some("Authentication service".to_string()));
        assert_eq!(service.capabilities.len(), 1);
        assert_eq!(service.tags.len(), 1);
    }

    #[test]
    fn test_parse_sidecar_minimal() {
        let content = r#"
files:
  test.rs:
    doc1: "Test file"
"#;

        let data = parse_sidecar(content).unwrap();

        assert!(data.directory_tags.is_empty());
        let test_file = data.files.get("test.rs").unwrap();
        assert_eq!(test_file.doc1, Some("Test file".to_string()));
    }

    #[test]
    fn test_parse_sidecar_empty() {
        let content = "{}";
        let data = parse_sidecar(content).unwrap();

        assert!(data.directory_tags.is_empty());
        assert!(data.files.is_empty());
    }

    #[test]
    fn test_extract_file_meta() {
        let content = r#"
files:
  service.rs:
    doc1: "Auth service"
    purpose: "Token handling"
    stability: stable
    owner: team-security
"#;

        let data = parse_sidecar(content).unwrap();
        let meta = extract_file_meta("src/auth/service.rs", &data, "src/auth").unwrap();

        assert_eq!(meta.file_path, "src/auth/service.rs");
        assert_eq!(meta.doc1, Some("Auth service".to_string()));
        assert_eq!(meta.stability, Some(Stability::Stable));
        assert_eq!(meta.owner, Some("team-security".to_string()));
        assert_eq!(meta.source, MetaSource::Sidecar);
        assert_eq!(meta.confidence, 1.0);
    }

    #[test]
    fn test_extract_file_tags() {
        let content = r#"
directory_tags:
  - domain:auth
  - layer:service

files:
  service.rs:
    tags:
      - pattern:idempotency
"#;

        let data = parse_sidecar(content).unwrap();
        let tags = extract_file_tags("src/auth/service.rs", &data);

        assert_eq!(tags.len(), 3);
        assert!(tags.contains(&"domain:auth".to_string()));
        assert!(tags.contains(&"layer:service".to_string()));
        assert!(tags.contains(&"pattern:idempotency".to_string()));
    }

    #[test]
    fn test_extract_file_tags_dedup() {
        let content = r#"
directory_tags:
  - domain:auth

files:
  service.rs:
    tags:
      - domain:auth
      - layer:service
"#;

        let data = parse_sidecar(content).unwrap();
        let tags = extract_file_tags("src/auth/service.rs", &data);

        // domain:auth should appear only once
        assert_eq!(tags.len(), 2);
        assert_eq!(tags.iter().filter(|t| *t == "domain:auth").count(), 1);
    }

    #[test]
    fn test_parse_tag() {
        assert_eq!(parse_tag("domain:auth"), (Some("domain"), "auth"));
        assert_eq!(parse_tag("auth"), (None, "auth"));
        assert_eq!(parse_tag("layer:service"), (Some("layer"), "service"));
    }

    #[test]
    fn test_find_sidecar_path() {
        assert_eq!(
            find_sidecar_path("src/auth/service.rs"),
            Some("src/auth/.code-indexer.yml".to_string())
        );

        assert_eq!(
            find_sidecar_path("lib.rs"),
            Some(".code-indexer.yml".to_string())
        );
    }

    #[test]
    fn test_extract_front_matter_rust() {
        let content = r#"//! @code-indexer
//! doc1: Authentication service
//! purpose: Handles JWT tokens
//! stability: stable

use crate::tokens::*;

pub fn authenticate() {}
"#;

        let meta = extract_front_matter(content, "rust").unwrap();
        assert_eq!(meta.doc1, Some("Authentication service".to_string()));
        assert_eq!(meta.purpose, Some("Handles JWT tokens".to_string()));
        assert_eq!(meta.stability, Some("stable".to_string()));
    }

    #[test]
    fn test_extract_front_matter_python() {
        let content = r#"# @code-indexer
# doc1: Database utility
# purpose: Connection pooling

import sqlite3
"#;

        let meta = extract_front_matter(content, "python").unwrap();
        assert_eq!(meta.doc1, Some("Database utility".to_string()));
    }

    #[test]
    fn test_extract_front_matter_no_marker() {
        let content = r#"//! This is a regular doc comment
//! No code-indexer marker here

use crate::something;
"#;

        let meta = extract_front_matter(content, "rust");
        assert!(meta.is_none());
    }

    #[test]
    fn test_resolve_tags() {
        let dict = vec![
            TagDictionary {
                id: 1,
                canonical_name: "auth".to_string(),
                category: "domain".to_string(),
                display_name: Some("Authentication".to_string()),
                synonyms: Some(vec!["authn".to_string()]),
            },
            TagDictionary {
                id: 2,
                canonical_name: "service".to_string(),
                category: "layer".to_string(),
                display_name: None,
                synonyms: None,
            },
        ];

        let tag_strings = vec!["domain:auth".to_string(), "service".to_string()];
        let resolved = resolve_tags("src/auth.rs", &tag_strings, &dict);

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].tag_id, 1);
        assert_eq!(resolved[0].tag_name, Some("auth".to_string()));
        assert_eq!(resolved[1].tag_id, 2);
    }

    #[test]
    fn test_resolve_tags_with_synonym() {
        let dict = vec![TagDictionary {
            id: 1,
            canonical_name: "auth".to_string(),
            category: "domain".to_string(),
            display_name: None,
            synonyms: Some(vec!["authn".to_string(), "login".to_string()]),
        }];

        let tag_strings = vec!["authn".to_string()];
        let resolved = resolve_tags("src/login.rs", &tag_strings, &dict);

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].tag_id, 1);
        assert_eq!(resolved[0].tag_name, Some("auth".to_string()));
    }

    #[test]
    fn test_compute_exported_hash_deterministic() {
        use crate::index::{Location, Symbol, SymbolKind, Visibility};

        let symbols = vec![
            Symbol::new("authenticate", SymbolKind::Function, Location::new("auth.rs", 1, 0, 10, 1), "rust")
                .with_visibility(Visibility::Public)
                .with_signature("fn authenticate(user: &str) -> bool"),
            Symbol::new("validate", SymbolKind::Function, Location::new("auth.rs", 15, 0, 25, 1), "rust")
                .with_visibility(Visibility::Public)
                .with_signature("fn validate(token: &str) -> Result<()>"),
        ];

        let hash1 = compute_exported_hash(&symbols);
        let hash2 = compute_exported_hash(&symbols);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 16); // 16 hex chars
    }

    #[test]
    fn test_compute_exported_hash_ignores_private() {
        use crate::index::{Location, Symbol, SymbolKind, Visibility};

        let public_only = vec![
            Symbol::new("public_fn", SymbolKind::Function, Location::new("test.rs", 1, 0, 5, 1), "rust")
                .with_visibility(Visibility::Public),
        ];

        let with_private = vec![
            Symbol::new("public_fn", SymbolKind::Function, Location::new("test.rs", 1, 0, 5, 1), "rust")
                .with_visibility(Visibility::Public),
            Symbol::new("private_fn", SymbolKind::Function, Location::new("test.rs", 10, 0, 15, 1), "rust")
                .with_visibility(Visibility::Private),
        ];

        let hash1 = compute_exported_hash(&public_only);
        let hash2 = compute_exported_hash(&with_private);

        // Hash should be same because private symbols are ignored
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_exported_hash_changes_on_signature_change() {
        use crate::index::{Location, Symbol, SymbolKind, Visibility};

        let v1 = vec![
            Symbol::new("process", SymbolKind::Function, Location::new("test.rs", 1, 0, 5, 1), "rust")
                .with_visibility(Visibility::Public)
                .with_signature("fn process(data: &[u8])"),
        ];

        let v2 = vec![
            Symbol::new("process", SymbolKind::Function, Location::new("test.rs", 1, 0, 5, 1), "rust")
                .with_visibility(Visibility::Public)
                .with_signature("fn process(data: &[u8], options: Options)"), // Added parameter
        ];

        let hash1 = compute_exported_hash(&v1);
        let hash2 = compute_exported_hash(&v2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_check_staleness() {
        use crate::index::{Location, Symbol, SymbolKind, Visibility};

        let symbols = vec![
            Symbol::new("test_fn", SymbolKind::Function, Location::new("test.rs", 1, 0, 5, 1), "rust")
                .with_visibility(Visibility::Public),
        ];

        let current_hash = compute_exported_hash(&symbols);

        // Fresh: stored hash matches current
        let (is_stale, hash) = check_staleness(&symbols, Some(&current_hash));
        assert!(!is_stale);
        assert_eq!(hash, current_hash);

        // Stale: stored hash differs
        let (is_stale, _) = check_staleness(&symbols, Some("different_hash"));
        assert!(is_stale);

        // No stored hash: not stale (can't determine)
        let (is_stale, _) = check_staleness(&symbols, None);
        assert!(!is_stale);
    }
}
