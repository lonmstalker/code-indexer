//! Versioned database migrations for SqliteIndex.
//!
//! Migrations are tracked in the `meta` table with key `schema_version`.
//! Each migration has a version number and runs exactly once.

use rusqlite::Connection;

use crate::error::{IndexerError, Result};

/// Current schema version. Increment when adding new migrations.
pub const CURRENT_SCHEMA_VERSION: u32 = 9;

/// Migration function type.
type MigrationFn = fn(&Connection) -> Result<()>;

/// All migrations in order. Index + 1 = version number.
const MIGRATIONS: &[MigrationFn] = &[
    migration_v1_base_schema,
    migration_v2_source_type_columns,
    migration_v3_scope_columns,
    migration_v4_stable_id,
    migration_v5_content_hash,
    migration_v6_file_tags_intent,
    migration_v7_p2_fields,
    migration_v8_definition_lookup_index,
    migration_v9_file_prefilter_metadata,
];

/// Runs all pending migrations on the database.
pub fn run_migrations(conn: &Connection) -> Result<()> {
    let current_version = get_schema_version(conn)?;

    for (idx, migration) in MIGRATIONS.iter().enumerate() {
        let version = (idx + 1) as u32;
        if version > current_version {
            migration(conn)?;
            set_schema_version(conn, version)?;
        }
    }

    Ok(())
}

/// Gets the current schema version from the database.
fn get_schema_version(conn: &Connection) -> Result<u32> {
    // First ensure meta table exists
    conn.execute(
        "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    )?;

    let version: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .ok();

    match version {
        Some(v) => Ok(v.parse().unwrap_or(0)),
        None => {
            // Check if db_revision exists (old schema) - if so, detect version from columns
            let has_revision = conn
                .query_row(
                    "SELECT value FROM meta WHERE key = 'db_revision'",
                    [],
                    |_| Ok(()),
                )
                .is_ok();

            if has_revision {
                // Detect version from existing columns
                detect_version_from_schema(conn)
            } else {
                Ok(0)
            }
        }
    }
}

/// Detects schema version by checking which columns exist.
/// Used for backward compatibility with databases created before versioning.
fn detect_version_from_schema(conn: &Connection) -> Result<u32> {
    // Check for incremental prefilter metadata columns (v9)
    if column_exists(conn, "files", "last_size")? && column_exists(conn, "files", "last_mtime_ns")?
    {
        return Ok(9);
    }

    // Check for definition lookup index (v8)
    if index_exists(conn, "idx_symbols_def_lookup")? {
        return Ok(8);
    }

    // Check for generic_params_json column (v7)
    if column_exists(conn, "symbols", "generic_params_json")? {
        return Ok(7);
    }

    // Check for file_tags table (v6)
    if table_exists(conn, "file_tags")? {
        return Ok(6);
    }

    // Check for stable_id (v4)
    let has_stable_id = column_exists(conn, "symbols", "stable_id")?;
    if has_stable_id {
        let has_content_hash = column_exists(conn, "files", "content_hash")?;
        if has_content_hash {
            return Ok(5);
        }
        return Ok(4);
    }

    // Check for scope_id (v3)
    let has_scope_id = column_exists(conn, "symbols", "scope_id")?;
    if has_scope_id {
        return Ok(3);
    }

    // Check for source_type (v2)
    let has_source_type = column_exists(conn, "symbols", "source_type")?;
    if has_source_type {
        return Ok(2);
    }

    // Check if symbols table exists (v1)
    let has_symbols = table_exists(conn, "symbols")?;
    if has_symbols {
        return Ok(1);
    }

    Ok(0)
}

/// Sets the schema version in the database.
fn set_schema_version(conn: &Connection, version: u32) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
        [version.to_string()],
    )?;
    Ok(())
}

/// Checks if a column exists in a table.
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = ?1",
            table
        ),
        [column],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Checks if a table exists.
fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Checks if an index exists.
fn index_exists(conn: &Connection, index: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
        [index],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Verifies that database schema version is compatible for read-only query operations.
/// Query paths do not run migrations and should fail fast on mismatched schema.
pub fn verify_schema_compatibility(conn: &Connection) -> Result<()> {
    let current = get_schema_version(conn)?;

    if current == 0 {
        return Err(IndexerError::Index(
            "Index database is not initialized. Run `code-indexer index` first.".to_string(),
        ));
    }

    if current > CURRENT_SCHEMA_VERSION {
        return Err(IndexerError::Index(format!(
            "Index schema version {} is newer than this binary ({}). Please upgrade code-indexer.",
            current, CURRENT_SCHEMA_VERSION
        )));
    }

    if current < CURRENT_SCHEMA_VERSION {
        return Err(IndexerError::Index(format!(
            "Index schema version {} is outdated (expected {}). Run `code-indexer index` to migrate.",
            current, CURRENT_SCHEMA_VERSION
        )));
    }

    Ok(())
}

// ============================================================================
// Migrations
// ============================================================================

/// V1: Base schema - all tables and FTS indexes.
fn migration_v1_base_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Symbols table
        CREATE TABLE IF NOT EXISTS symbols (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            file_path TEXT NOT NULL,
            start_line INTEGER NOT NULL,
            start_column INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            end_column INTEGER NOT NULL,
            language TEXT NOT NULL,
            visibility TEXT,
            signature TEXT,
            doc_comment TEXT,
            parent TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
        CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind);
        CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_path);
        CREATE INDEX IF NOT EXISTS idx_symbols_language ON symbols(language);
        CREATE INDEX IF NOT EXISTS idx_symbols_parent ON symbols(parent);
        CREATE INDEX IF NOT EXISTS idx_symbols_file_line ON symbols(file_path, start_line);

        -- FTS5 index for full-text search
        CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(
            name,
            signature,
            doc_comment,
            content='symbols',
            content_rowid='rowid'
        );

        -- FTS triggers for automatic sync
        CREATE TRIGGER IF NOT EXISTS symbols_ai AFTER INSERT ON symbols BEGIN
            INSERT INTO symbols_fts(rowid, name, signature, doc_comment)
            VALUES (new.rowid, new.name, new.signature, new.doc_comment);
        END;

        CREATE TRIGGER IF NOT EXISTS symbols_ad AFTER DELETE ON symbols BEGIN
            INSERT INTO symbols_fts(symbols_fts, rowid, name, signature, doc_comment)
            VALUES ('delete', old.rowid, old.name, old.signature, old.doc_comment);
        END;

        CREATE TRIGGER IF NOT EXISTS symbols_au AFTER UPDATE ON symbols BEGIN
            INSERT INTO symbols_fts(symbols_fts, rowid, name, signature, doc_comment)
            VALUES ('delete', old.rowid, old.name, old.signature, old.doc_comment);
            INSERT INTO symbols_fts(rowid, name, signature, doc_comment)
            VALUES (new.rowid, new.name, new.signature, new.doc_comment);
        END;

        -- Files table
        CREATE TABLE IF NOT EXISTS files (
            path TEXT PRIMARY KEY,
            language TEXT NOT NULL,
            last_modified INTEGER NOT NULL,
            symbol_count INTEGER NOT NULL DEFAULT 0
        );

        -- Projects table
        CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            version TEXT,
            ecosystem TEXT NOT NULL,
            manifest_path TEXT NOT NULL UNIQUE
        );

        -- Dependencies table
        CREATE TABLE IF NOT EXISTS dependencies (
            id INTEGER PRIMARY KEY,
            project_id INTEGER REFERENCES projects(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            version TEXT NOT NULL,
            ecosystem TEXT NOT NULL,
            source_path TEXT,
            is_dev INTEGER DEFAULT 0,
            is_indexed INTEGER DEFAULT 0,
            UNIQUE(project_id, name, version)
        );

        CREATE INDEX IF NOT EXISTS idx_dependencies_project ON dependencies(project_id);
        CREATE INDEX IF NOT EXISTS idx_dependencies_name ON dependencies(name);

        -- Symbol references table
        CREATE TABLE IF NOT EXISTS symbol_references (
            id INTEGER PRIMARY KEY,
            symbol_id TEXT REFERENCES symbols(id),
            symbol_name TEXT NOT NULL,
            referenced_in_file TEXT NOT NULL,
            line INTEGER NOT NULL,
            column_num INTEGER NOT NULL,
            reference_kind TEXT NOT NULL,
            UNIQUE(symbol_name, referenced_in_file, line, column_num)
        );

        CREATE INDEX IF NOT EXISTS idx_refs_symbol_id ON symbol_references(symbol_id);
        CREATE INDEX IF NOT EXISTS idx_refs_symbol_name ON symbol_references(symbol_name);
        CREATE INDEX IF NOT EXISTS idx_refs_file ON symbol_references(referenced_in_file);
        CREATE INDEX IF NOT EXISTS idx_refs_kind ON symbol_references(reference_kind);
        CREATE INDEX IF NOT EXISTS idx_refs_symbol_kind ON symbol_references(symbol_name, reference_kind);

        -- File imports table
        CREATE TABLE IF NOT EXISTS file_imports (
            id INTEGER PRIMARY KEY,
            file_path TEXT NOT NULL,
            imported_path TEXT,
            imported_symbol TEXT,
            import_type TEXT NOT NULL,
            UNIQUE(file_path, imported_path, imported_symbol)
        );

        CREATE INDEX IF NOT EXISTS idx_imports_file ON file_imports(file_path);
        CREATE INDEX IF NOT EXISTS idx_imports_path ON file_imports(imported_path);
        CREATE INDEX IF NOT EXISTS idx_imports_symbol ON file_imports(imported_symbol);

        -- Scopes table
        CREATE TABLE IF NOT EXISTS scopes (
            id INTEGER PRIMARY KEY,
            file_path TEXT NOT NULL,
            parent_id INTEGER REFERENCES scopes(id),
            kind TEXT NOT NULL,
            name TEXT,
            start_offset INTEGER NOT NULL,
            end_offset INTEGER NOT NULL,
            start_line INTEGER NOT NULL,
            end_line INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_scopes_file ON scopes(file_path);
        CREATE INDEX IF NOT EXISTS idx_scopes_range ON scopes(file_path, start_offset, end_offset);

        -- Call edges table
        CREATE TABLE IF NOT EXISTS call_edges (
            id INTEGER PRIMARY KEY,
            caller_symbol_id TEXT NOT NULL REFERENCES symbols(id),
            callee_symbol_id TEXT REFERENCES symbols(id),
            callee_name TEXT NOT NULL,
            file_path TEXT NOT NULL,
            line INTEGER NOT NULL,
            column_num INTEGER NOT NULL,
            confidence TEXT NOT NULL DEFAULT 'certain',
            reason TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_calls_caller ON call_edges(caller_symbol_id);
        CREATE INDEX IF NOT EXISTS idx_calls_callee ON call_edges(callee_symbol_id);
        CREATE INDEX IF NOT EXISTS idx_calls_callee_name ON call_edges(callee_name);

        -- Symbol metrics table
        CREATE TABLE IF NOT EXISTS symbol_metrics (
            symbol_id TEXT PRIMARY KEY REFERENCES symbols(id),
            pagerank REAL DEFAULT 0.0,
            incoming_refs INTEGER DEFAULT 0,
            outgoing_refs INTEGER DEFAULT 0,
            git_recency REAL DEFAULT 0.0
        );

        -- Meta table
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        INSERT OR IGNORE INTO meta (key, value) VALUES ('db_revision', '0');

        -- Documentation digests table
        CREATE TABLE IF NOT EXISTS doc_digests (
            id INTEGER PRIMARY KEY,
            file_path TEXT NOT NULL UNIQUE,
            doc_type TEXT NOT NULL,
            title TEXT,
            headings TEXT,
            command_blocks TEXT,
            key_sections TEXT,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_doc_digests_type ON doc_digests(doc_type);

        -- Configuration digests table
        CREATE TABLE IF NOT EXISTS config_digests (
            id INTEGER PRIMARY KEY,
            file_path TEXT NOT NULL UNIQUE,
            config_type TEXT NOT NULL,
            name TEXT,
            version TEXT,
            scripts TEXT,
            build_targets TEXT,
            test_commands TEXT,
            run_commands TEXT,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_config_digests_type ON config_digests(config_type);

        -- Project profile table
        CREATE TABLE IF NOT EXISTS project_profile (
            id INTEGER PRIMARY KEY,
            project_path TEXT NOT NULL UNIQUE,
            languages TEXT NOT NULL,
            frameworks TEXT,
            build_tools TEXT,
            workspace_type TEXT,
            profile_rev INTEGER DEFAULT 0,
            updated_at INTEGER NOT NULL
        );

        -- Project nodes table
        CREATE TABLE IF NOT EXISTS project_nodes (
            id TEXT PRIMARY KEY,
            parent_id TEXT,
            node_type TEXT NOT NULL,
            name TEXT NOT NULL,
            path TEXT NOT NULL,
            symbol_count INTEGER DEFAULT 0,
            public_symbol_count INTEGER DEFAULT 0,
            file_count INTEGER DEFAULT 0,
            centrality_score REAL DEFAULT 0.0
        );
        CREATE INDEX IF NOT EXISTS idx_nodes_parent ON project_nodes(parent_id);
        CREATE INDEX IF NOT EXISTS idx_nodes_path ON project_nodes(path);

        -- Entry points table
        CREATE TABLE IF NOT EXISTS entry_points (
            id INTEGER PRIMARY KEY,
            symbol_id TEXT,
            entry_type TEXT NOT NULL,
            file_path TEXT NOT NULL,
            line INTEGER NOT NULL,
            name TEXT NOT NULL,
            evidence TEXT,
            UNIQUE(file_path, line)
        );
        CREATE INDEX IF NOT EXISTS idx_entry_type ON entry_points(entry_type);

        -- Sessions table
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL,
            last_accessed INTEGER NOT NULL,
            metadata TEXT
        );

        -- Session dictionary table
        CREATE TABLE IF NOT EXISTS session_dict (
            session_id TEXT NOT NULL,
            key_type TEXT NOT NULL,
            full_value TEXT NOT NULL,
            short_id INTEGER NOT NULL,
            PRIMARY KEY (session_id, key_type, full_value)
        );
        CREATE INDEX IF NOT EXISTS idx_session_dict_session ON session_dict(session_id);
        "#,
    )?;
    Ok(())
}

/// V2: Add source_type and dependency_id columns for dependency tracking.
fn migration_v2_source_type_columns(conn: &Connection) -> Result<()> {
    if !column_exists(conn, "symbols", "source_type")? {
        conn.execute(
            "ALTER TABLE symbols ADD COLUMN source_type TEXT DEFAULT 'project'",
            [],
        )?;
        conn.execute(
            "ALTER TABLE symbols ADD COLUMN dependency_id INTEGER REFERENCES dependencies(id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_source_type ON symbols(source_type)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_dependency ON symbols(dependency_id)",
            [],
        )?;
    }
    Ok(())
}

/// V3: Add scope_id and fqdn columns for scope resolution.
fn migration_v3_scope_columns(conn: &Connection) -> Result<()> {
    if !column_exists(conn, "symbols", "scope_id")? {
        conn.execute(
            "ALTER TABLE symbols ADD COLUMN scope_id INTEGER REFERENCES scopes(id)",
            [],
        )?;
        conn.execute("ALTER TABLE symbols ADD COLUMN fqdn TEXT", [])?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_scope ON symbols(scope_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_fqdn ON symbols(fqdn)",
            [],
        )?;
    }
    Ok(())
}

/// V4: Add stable_id column for summary-first contract.
fn migration_v4_stable_id(conn: &Connection) -> Result<()> {
    if !column_exists(conn, "symbols", "stable_id")? {
        conn.execute("ALTER TABLE symbols ADD COLUMN stable_id TEXT", [])?;
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_symbols_stable_id ON symbols(stable_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_cursor ON symbols(kind, file_path, start_line, id)",
            [],
        )?;
    }
    Ok(())
}

/// V5: Add content_hash column for incremental indexing.
fn migration_v5_content_hash(conn: &Connection) -> Result<()> {
    if !column_exists(conn, "files", "content_hash")? {
        conn.execute("ALTER TABLE files ADD COLUMN content_hash TEXT", [])?;
    }
    Ok(())
}

/// V6: File tags and intent layer for agent-friendly metadata.
fn migration_v6_file_tags_intent(conn: &Connection) -> Result<()> {
    // Tag dictionary for normalization and taxonomy
    if !table_exists(conn, "tag_dictionary")? {
        conn.execute_batch(
            r#"
            CREATE TABLE tag_dictionary (
                id INTEGER PRIMARY KEY,
                canonical_name TEXT NOT NULL UNIQUE,
                category TEXT NOT NULL,
                display_name TEXT,
                synonyms TEXT
            );
            CREATE INDEX idx_tag_dict_category ON tag_dictionary(category);

            -- Seed common tags
            INSERT INTO tag_dictionary (canonical_name, category, display_name, synonyms) VALUES
                ('auth', 'domain', 'Authentication', '["authn","login","sso"]'),
                ('payments', 'domain', 'Payments', '["billing","checkout"]'),
                ('api', 'layer', 'API Layer', '["rest","handler","controller"]'),
                ('service', 'layer', 'Service Layer', '["business","usecase"]'),
                ('repository', 'layer', 'Repository', '["dao","store"]'),
                ('model', 'layer', 'Model Layer', '["entity","dto"]'),
                ('idempotency', 'pattern', 'Idempotency', '["idempotent"]'),
                ('cache', 'pattern', 'Caching', '["memoize"]'),
                ('retry', 'pattern', 'Retry Logic', '["backoff"]'),
                ('test', 'infra', 'Testing', '["spec","fixture"]'),
                ('config', 'infra', 'Configuration', '["settings","env"]'),
                ('cli', 'infra', 'CLI', '["command","args"]'),
                ('mcp', 'infra', 'MCP Server', '["tool","server"]');
            "#,
        )?;
    }

    // File-level metadata (Intent Layer)
    if !table_exists(conn, "file_meta")? {
        conn.execute_batch(
            r#"
            CREATE TABLE file_meta (
                file_path TEXT PRIMARY KEY,
                doc1 TEXT,
                purpose TEXT,
                capabilities TEXT,
                invariants TEXT,
                non_goals TEXT,
                security_notes TEXT,
                owner TEXT,
                stability TEXT,
                exported_hash TEXT,
                last_extracted INTEGER NOT NULL,
                source TEXT NOT NULL DEFAULT 'inferred',
                confidence REAL DEFAULT 1.0
            );
            CREATE INDEX idx_file_meta_owner ON file_meta(owner);
            CREATE INDEX idx_file_meta_stability ON file_meta(stability);
            CREATE INDEX idx_file_meta_source ON file_meta(source);
            "#,
        )?;
    }

    // File tags (many-to-many)
    if !table_exists(conn, "file_tags")? {
        conn.execute_batch(
            r#"
            CREATE TABLE file_tags (
                id INTEGER PRIMARY KEY,
                file_path TEXT NOT NULL,
                tag_id INTEGER NOT NULL REFERENCES tag_dictionary(id),
                source TEXT NOT NULL DEFAULT 'inferred',
                confidence REAL DEFAULT 1.0,
                reason TEXT,
                UNIQUE(file_path, tag_id)
            );
            CREATE INDEX idx_file_tags_file ON file_tags(file_path);
            CREATE INDEX idx_file_tags_tag ON file_tags(tag_id);
            CREATE INDEX idx_file_tags_source ON file_tags(source);
            "#,
        )?;
    }

    // FTS for searching doc1/purpose
    let has_fts: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='file_meta_fts'",
        [],
        |row| row.get(0),
    )?;
    if has_fts == 0 {
        conn.execute_batch(
            r#"
            CREATE VIRTUAL TABLE file_meta_fts USING fts5(
                doc1, purpose,
                content='file_meta',
                content_rowid='rowid'
            );

            -- FTS triggers for automatic sync
            CREATE TRIGGER file_meta_ai AFTER INSERT ON file_meta BEGIN
                INSERT INTO file_meta_fts(rowid, doc1, purpose)
                VALUES (new.rowid, new.doc1, new.purpose);
            END;

            CREATE TRIGGER file_meta_ad AFTER DELETE ON file_meta BEGIN
                INSERT INTO file_meta_fts(file_meta_fts, rowid, doc1, purpose)
                VALUES ('delete', old.rowid, old.doc1, old.purpose);
            END;

            CREATE TRIGGER file_meta_au AFTER UPDATE ON file_meta BEGIN
                INSERT INTO file_meta_fts(file_meta_fts, rowid, doc1, purpose)
                VALUES ('delete', old.rowid, old.doc1, old.purpose);
                INSERT INTO file_meta_fts(rowid, doc1, purpose)
                VALUES (new.rowid, new.doc1, new.purpose);
            END;
            "#,
        )?;
    }

    Ok(())
}

/// V7: Add P2 fields for generic params, function params, and return types.
fn migration_v7_p2_fields(conn: &Connection) -> Result<()> {
    // Add generic_params_json column for storing generic type parameters as JSON
    if !column_exists(conn, "symbols", "generic_params_json")? {
        conn.execute(
            "ALTER TABLE symbols ADD COLUMN generic_params_json TEXT",
            [],
        )?;
    }

    // Add params_json column for storing function parameters as JSON
    if !column_exists(conn, "symbols", "params_json")? {
        conn.execute("ALTER TABLE symbols ADD COLUMN params_json TEXT", [])?;
    }

    // Add return_type column for storing function return types
    if !column_exists(conn, "symbols", "return_type")? {
        conn.execute("ALTER TABLE symbols ADD COLUMN return_type TEXT", [])?;
    }

    Ok(())
}

/// V8: Add composite index optimized for definition lookups.
fn migration_v8_definition_lookup_index(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbols_def_lookup ON symbols(name, file_path, start_line)",
        [],
    )?;
    Ok(())
}

/// V9: Add cheap incremental prefilter metadata to files table.
fn migration_v9_file_prefilter_metadata(conn: &Connection) -> Result<()> {
    if !column_exists(conn, "files", "last_size")? {
        conn.execute("ALTER TABLE files ADD COLUMN last_size INTEGER", [])?;
    }
    if !column_exists(conn, "files", "last_mtime_ns")? {
        conn.execute("ALTER TABLE files ADD COLUMN last_mtime_ns INTEGER", [])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_fresh_database_migrations() {
        let conn = Connection::open_in_memory().unwrap();

        run_migrations(&conn).unwrap();

        // Verify schema version
        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);

        // Verify tables exist
        assert!(table_exists(&conn, "symbols").unwrap());
        assert!(table_exists(&conn, "files").unwrap());
        assert!(table_exists(&conn, "meta").unwrap());
        assert!(column_exists(&conn, "files", "last_size").unwrap());
        assert!(column_exists(&conn, "files", "last_mtime_ns").unwrap());
    }

    #[test]
    fn test_migrations_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // Run twice
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();

        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_detect_version_from_schema() {
        let conn = Connection::open_in_memory().unwrap();

        // Empty database
        assert_eq!(detect_version_from_schema(&conn).unwrap(), 0);

        // Create v1 schema
        migration_v1_base_schema(&conn).unwrap();
        assert!(detect_version_from_schema(&conn).unwrap() >= 1);
    }

    #[test]
    fn test_column_exists() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE test (id INTEGER, name TEXT)", [])
            .unwrap();

        assert!(column_exists(&conn, "test", "id").unwrap());
        assert!(column_exists(&conn, "test", "name").unwrap());
        assert!(!column_exists(&conn, "test", "nonexistent").unwrap());
    }

    #[test]
    fn test_table_exists() {
        let conn = Connection::open_in_memory().unwrap();

        assert!(!table_exists(&conn, "test").unwrap());

        conn.execute("CREATE TABLE test (id INTEGER)", []).unwrap();
        assert!(table_exists(&conn, "test").unwrap());
    }

    #[test]
    fn test_migration_v6_file_tags() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Verify tag_dictionary exists and has seed data
        let tag_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tag_dictionary", [], |row| row.get(0))
            .unwrap();
        assert!(tag_count > 0, "tag_dictionary should have seed data");

        // Verify file_meta table exists
        assert!(table_exists(&conn, "file_meta").unwrap());

        // Verify file_tags table exists
        assert!(table_exists(&conn, "file_tags").unwrap());

        // Verify FTS index exists
        let fts_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='file_meta_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_exists, 1, "file_meta_fts should exist");

        // Test inserting file_meta
        conn.execute(
            "INSERT INTO file_meta (file_path, doc1, purpose, last_extracted, source) VALUES (?1, ?2, ?3, ?4, ?5)",
            ["src/auth/service.rs", "Auth service", "Handles authentication", "1234567890", "sidecar"],
        )
        .unwrap();

        // Test FTS search works
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM file_meta_fts WHERE file_meta_fts MATCH 'auth'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, 1, "FTS search should find the inserted row");
    }

    #[test]
    fn test_migration_v7_p2_fields() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Verify P2 columns exist in symbols table
        assert!(
            column_exists(&conn, "symbols", "generic_params_json").unwrap(),
            "generic_params_json column should exist"
        );
        assert!(
            column_exists(&conn, "symbols", "params_json").unwrap(),
            "params_json column should exist"
        );
        assert!(
            column_exists(&conn, "symbols", "return_type").unwrap(),
            "return_type column should exist"
        );

        // Test inserting symbol with P2 fields
        conn.execute(
            r#"INSERT INTO symbols
            (id, name, kind, file_path, start_line, start_column, end_line, end_column,
             language, generic_params_json, params_json, return_type)
            VALUES ('test-id', 'Result', 'enum', 'test.rs', 1, 0, 10, 1, 'rust',
                    '[{"name":"T","bounds":[],"default":null},{"name":"E","bounds":["Error"],"default":null}]',
                    '[]',
                    'Self')"#,
            [],
        )
        .unwrap();

        // Verify data was inserted correctly
        let generic_params: String = conn
            .query_row(
                "SELECT generic_params_json FROM symbols WHERE id = 'test-id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(generic_params.contains("\"name\":\"T\""));
        assert!(generic_params.contains("\"name\":\"E\""));
    }

    #[test]
    fn test_verify_schema_compatibility() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        verify_schema_compatibility(&conn).unwrap();

        conn.execute(
            "UPDATE meta SET value = '7' WHERE key = 'schema_version'",
            [],
        )
        .unwrap();
        assert!(verify_schema_compatibility(&conn).is_err());
    }
}
