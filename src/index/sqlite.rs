use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

use crate::dependencies::{Dependency, Ecosystem, ProjectInfo, SymbolSource};
use crate::error::Result;
use crate::index::{
    CallConfidence, CallGraph, CallGraphEdge, CallGraphNode, CodeIndex, DeadCodeReport, FileImport,
    FunctionMetrics, ImportType, IndexStats, PaginationCursor, ReferenceKind, Scope, ScopeKind,
    SearchOptions, SearchResult, Symbol, SymbolKind, SymbolMetrics, SymbolReference,
    UncertaintyReason, Visibility,
};
use crate::indexer::ExtractionResult;

pub struct SqliteIndex {
    conn: Mutex<Connection>,
}

impl SqliteIndex {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        Self::configure_pragmas(&conn)?;
        let index = Self {
            conn: Mutex::new(conn),
        };
        index.init_schema()?;
        Ok(index)
    }

    #[allow(dead_code)]
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::configure_pragmas(&conn)?;
        let index = Self {
            conn: Mutex::new(conn),
        };
        index.init_schema()?;
        Ok(index)
    }

    /// Configure SQLite PRAGMA settings for optimal performance.
    /// - WAL mode: allows concurrent reads during writes
    /// - NORMAL synchronous: good durability with better performance
    /// - 64MB cache: reduces disk I/O for large indexes
    /// - MEMORY temp_store: speeds up temporary tables and sorts
    fn configure_pragmas(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = -64000;
            PRAGMA temp_store = MEMORY;
            "#,
        )?;
        Ok(())
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
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
            -- Index for parent lookups (get children of a type/module)
            CREATE INDEX IF NOT EXISTS idx_symbols_parent ON symbols(parent);
            -- Composite index for location-based queries (go-to-definition, file outline)
            CREATE INDEX IF NOT EXISTS idx_symbols_file_line ON symbols(file_path, start_line);

            CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(
                name,
                signature,
                doc_comment,
                content='symbols',
                content_rowid='rowid'
            );

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

            CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                language TEXT NOT NULL,
                last_modified INTEGER NOT NULL,
                symbol_count INTEGER NOT NULL DEFAULT 0,
                content_hash TEXT
            );

            -- Projects table for tracking indexed projects
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

            -- Symbol references table for tracking usages
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
            -- Composite index for efficient symbol+kind queries (call graph, references by type)
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

            -- Scopes table for scope resolution
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

            -- Call edges table with confidence
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

            -- Symbol metrics table for ranking
            CREATE TABLE IF NOT EXISTS symbol_metrics (
                symbol_id TEXT PRIMARY KEY REFERENCES symbols(id),
                pagerank REAL DEFAULT 0.0,
                incoming_refs INTEGER DEFAULT 0,
                outgoing_refs INTEGER DEFAULT 0,
                git_recency REAL DEFAULT 0.0
            );

            -- Meta table for tracking database revision and other global state
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            -- Initialize db_revision if it doesn't exist
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

            -- Project nodes table for module hierarchy
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

        // Add source_type and dependency_id columns if they don't exist
        // Using a migration-style approach (inline to avoid deadlock)
        let has_source_type: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('symbols') WHERE name = 'source_type'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false);

        if !has_source_type {
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

        // Add scope_id and fqdn columns if they don't exist
        let has_scope_id: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('symbols') WHERE name = 'scope_id'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false);

        if !has_scope_id {
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

        // Add stable_id column if it doesn't exist (for summary-first contract)
        let has_stable_id: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('symbols') WHERE name = 'stable_id'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false);

        if !has_stable_id {
            conn.execute("ALTER TABLE symbols ADD COLUMN stable_id TEXT", [])?;
            // Unique index for stable_id
            conn.execute(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_symbols_stable_id ON symbols(stable_id)",
                [],
            )?;
            // Composite index for cursor-based pagination: (kind, file_path, start_line, id)
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_symbols_cursor ON symbols(kind, file_path, start_line, id)",
                [],
            )?;
        }

        // Add content_hash column if it doesn't exist (for incremental indexing)
        let has_content_hash: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('files') WHERE name = 'content_hash'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false);

        if !has_content_hash {
            conn.execute("ALTER TABLE files ADD COLUMN content_hash TEXT", [])?;
        }

        Ok(())
    }

    // === Database Revision Methods (Summary-First Contract) ===

    /// Gets the current database revision number.
    /// This is a monotonic counter incremented after each write transaction.
    pub fn get_db_revision(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let rev: String = conn.query_row(
            "SELECT value FROM meta WHERE key = 'db_revision'",
            [],
            |row| row.get(0),
        )?;
        Ok(rev.parse().unwrap_or(0))
    }

    /// Increments and returns the new database revision number.
    /// Should be called after each write transaction.
    pub fn increment_db_revision(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE meta SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT) WHERE key = 'db_revision'",
            [],
        )?;
        let rev: String = conn.query_row(
            "SELECT value FROM meta WHERE key = 'db_revision'",
            [],
            |row| row.get(0),
        )?;
        Ok(rev.parse().unwrap_or(0))
    }

    // === Incremental Indexing Methods ===

    /// Gets the stored content hash for a file.
    /// Returns None if the file is not in the index.
    pub fn get_file_content_hash(&self, file_path: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let hash: Option<String> = conn
            .query_row(
                "SELECT content_hash FROM files WHERE path = ?1",
                params![file_path],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(hash)
    }

    /// Updates the content hash for a file.
    pub fn set_file_content_hash(&self, file_path: &str, content_hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE files SET content_hash = ?1 WHERE path = ?2",
            params![content_hash, file_path],
        )?;
        Ok(())
    }

    /// Checks if a file needs reindexing based on content hash.
    /// Returns true if:
    /// - File is not in the index
    /// - Content hash is different
    /// - Content hash is not stored (null)
    pub fn file_needs_reindex(&self, file_path: &str, new_content_hash: &str) -> Result<bool> {
        match self.get_file_content_hash(file_path)? {
            Some(stored_hash) => Ok(stored_hash != new_content_hash),
            None => Ok(true), // File not indexed or hash not stored
        }
    }

    /// Computes SHA256 hash of file content.
    pub fn compute_content_hash(content: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    // === Project and Dependency Methods ===

    /// Adds or updates a project in the database.
    pub fn add_project(&self, project: &ProjectInfo) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO projects (name, version, ecosystem, manifest_path)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![
                project.name,
                project.version,
                project.ecosystem.as_str(),
                project.manifest_path,
            ],
        )?;

        let project_id = conn.last_insert_rowid();
        Ok(project_id)
    }

    /// Gets a project by its manifest path.
    pub fn get_project(&self, manifest_path: &str) -> Result<Option<ProjectInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, version, ecosystem, manifest_path
            FROM projects WHERE manifest_path = ?1
            "#,
        )?;

        let project = stmt
            .query_row(params![manifest_path], |row| {
                let ecosystem_str: String = row.get(3)?;
                Ok(ProjectInfo {
                    name: row.get(1)?,
                    version: row.get(2)?,
                    ecosystem: Ecosystem::from_str(&ecosystem_str).unwrap_or(Ecosystem::Cargo),
                    manifest_path: row.get(4)?,
                    dependencies: Vec::new(),
                })
            })
            .optional()?;

        Ok(project)
    }

    /// Gets the project ID by manifest path.
    pub fn get_project_id(&self, manifest_path: &str) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        let id: Option<i64> = conn
            .query_row(
                "SELECT id FROM projects WHERE manifest_path = ?1",
                params![manifest_path],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id)
    }

    /// Adds dependencies for a project.
    pub fn add_dependencies(&self, project_id: i64, dependencies: &[Dependency]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        for dep in dependencies {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO dependencies
                (project_id, name, version, ecosystem, source_path, is_dev, is_indexed)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    project_id,
                    dep.name,
                    dep.version,
                    dep.ecosystem.as_str(),
                    dep.source_path,
                    dep.is_dev as i32,
                    dep.is_indexed as i32,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Gets all dependencies for a project.
    pub fn get_dependencies(&self, project_id: i64, include_dev: bool) -> Result<Vec<Dependency>> {
        let conn = self.conn.lock().unwrap();

        let sql = if include_dev {
            "SELECT name, version, ecosystem, source_path, is_dev, is_indexed FROM dependencies WHERE project_id = ?1"
        } else {
            "SELECT name, version, ecosystem, source_path, is_dev, is_indexed FROM dependencies WHERE project_id = ?1 AND is_dev = 0"
        };

        let mut stmt = conn.prepare(sql)?;
        let deps = stmt
            .query_map(params![project_id], |row| {
                let ecosystem_str: String = row.get(2)?;
                let is_dev: i32 = row.get(4)?;
                let is_indexed: i32 = row.get(5)?;

                Ok(Dependency {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    ecosystem: Ecosystem::from_str(&ecosystem_str).unwrap_or(Ecosystem::Cargo),
                    source_path: row.get(3)?,
                    is_dev: is_dev != 0,
                    is_indexed: is_indexed != 0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(deps)
    }

    /// Gets a dependency by name for a project.
    pub fn get_dependency(&self, project_id: i64, name: &str) -> Result<Option<Dependency>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT name, version, ecosystem, source_path, is_dev, is_indexed
            FROM dependencies WHERE project_id = ?1 AND name = ?2
            "#,
        )?;

        let dep = stmt
            .query_row(params![project_id, name], |row| {
                let ecosystem_str: String = row.get(2)?;
                let is_dev: i32 = row.get(4)?;
                let is_indexed: i32 = row.get(5)?;

                Ok(Dependency {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    ecosystem: Ecosystem::from_str(&ecosystem_str).unwrap_or(Ecosystem::Cargo),
                    source_path: row.get(3)?,
                    is_dev: is_dev != 0,
                    is_indexed: is_indexed != 0,
                })
            })
            .optional()?;

        Ok(dep)
    }

    /// Gets the dependency ID by project and name.
    pub fn get_dependency_id(&self, project_id: i64, name: &str) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        let id: Option<i64> = conn
            .query_row(
                "SELECT id FROM dependencies WHERE project_id = ?1 AND name = ?2",
                params![project_id, name],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id)
    }

    /// Marks a dependency as indexed.
    pub fn mark_dependency_indexed(&self, dep_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE dependencies SET is_indexed = 1 WHERE id = ?1",
            params![dep_id],
        )?;
        Ok(())
    }

    /// Adds symbols from a dependency.
    pub fn add_dependency_symbols(&self, dep_id: i64, symbols: Vec<Symbol>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        for symbol in symbols {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO symbols
                (id, name, kind, file_path, start_line, start_column, end_line, end_column,
                 language, visibility, signature, doc_comment, parent, source_type, dependency_id)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'dependency', ?14)
                "#,
                params![
                    symbol.id,
                    symbol.name,
                    symbol.kind.as_str(),
                    symbol.location.file_path,
                    symbol.location.start_line,
                    symbol.location.start_column,
                    symbol.location.end_line,
                    symbol.location.end_column,
                    symbol.language,
                    symbol.visibility.as_ref().map(|v| v.as_str()),
                    symbol.signature,
                    symbol.doc_comment,
                    symbol.parent,
                    dep_id,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Searches symbols in dependencies only.
    pub fn search_in_dependencies(
        &self,
        query: &str,
        dep_name: Option<&str>,
        options: &SearchOptions,
    ) -> Result<Vec<SearchResult>> {
        let conn = self.conn.lock().unwrap();
        let limit = options.limit.unwrap_or(100);

        let fts_query = format!("{}*", query.replace(['*', '"', '\''], ""));

        let mut sql = String::from(
            r#"
            SELECT s.id, s.name, s.kind, s.file_path, s.start_line, s.start_column,
                   s.end_line, s.end_column, s.language, s.visibility, s.signature,
                   s.doc_comment, s.parent, bm25(symbols_fts) as score
            FROM symbols s
            JOIN symbols_fts ON s.rowid = symbols_fts.rowid
            WHERE symbols_fts MATCH ?1 AND s.source_type = 'dependency'
            "#,
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(fts_query)];

        if let Some(name) = dep_name {
            sql.push_str(
                " AND s.dependency_id IN (SELECT id FROM dependencies WHERE name = ?)",
            );
            params_vec.push(Box::new(name.to_string()));
        }

        if let Some(ref kinds) = options.kind_filter {
            let placeholders: Vec<String> = kinds.iter().map(|_| "?".to_string()).collect();
            sql.push_str(&format!(" AND s.kind IN ({})", placeholders.join(",")));
            for kind in kinds {
                params_vec.push(Box::new(kind.as_str().to_string()));
            }
        }

        if let Some(ref langs) = options.language_filter {
            let placeholders: Vec<String> = langs.iter().map(|_| "?".to_string()).collect();
            sql.push_str(&format!(" AND s.language IN ({})", placeholders.join(",")));
            for lang in langs {
                params_vec.push(Box::new(lang.clone()));
            }
        }

        sql.push_str(&format!(" ORDER BY score LIMIT {}", limit));

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let results = stmt
            .query_map(params_refs.as_slice(), |row| {
                let symbol = Self::symbol_from_row(row)?;
                let score: f64 = row.get(13)?;
                Ok(SearchResult {
                    symbol,
                    score: -score,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(results)
    }

    /// Finds a symbol definition in dependencies.
    pub fn find_definition_in_dependencies(
        &self,
        name: &str,
        dep_name: Option<&str>,
    ) -> Result<Vec<Symbol>> {
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from(
            r#"
            SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                   language, visibility, signature, doc_comment, parent
            FROM symbols
            WHERE name = ?1 AND kind NOT IN ('import', 'variable') AND source_type = 'dependency'
            "#,
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(name.to_string())];

        if let Some(dep) = dep_name {
            sql.push_str(
                " AND dependency_id IN (SELECT id FROM dependencies WHERE name = ?)",
            );
            params_vec.push(Box::new(dep.to_string()));
        }

        sql.push_str(" ORDER BY file_path, start_line");

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let symbols = stmt
            .query_map(params_refs.as_slice(), Self::symbol_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(symbols)
    }

    /// Gets the symbol source information.
    pub fn get_symbol_source(&self, symbol_id: &str) -> Result<Option<SymbolSource>> {
        let conn = self.conn.lock().unwrap();

        let result: Option<(String, Option<i64>)> = conn
            .query_row(
                "SELECT source_type, dependency_id FROM symbols WHERE id = ?1",
                params![symbol_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        if let Some((source_type, dep_id)) = result {
            if source_type == "dependency" {
                if let Some(id) = dep_id {
                    let dep_info: Option<(String, String, String)> = conn
                        .query_row(
                            "SELECT name, version, ecosystem FROM dependencies WHERE id = ?1",
                            params![id],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                        )
                        .optional()?;

                    if let Some((name, version, ecosystem_str)) = dep_info {
                        return Ok(Some(SymbolSource::Dependency {
                            name,
                            version,
                            ecosystem: Ecosystem::from_str(&ecosystem_str)
                                .unwrap_or(Ecosystem::Cargo),
                        }));
                    }
                }
            }
            return Ok(Some(SymbolSource::Project));
        }

        Ok(None)
    }

    /// Removes all symbols from a dependency.
    pub fn remove_dependency_symbols(&self, dep_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM symbols WHERE dependency_id = ?1",
            params![dep_id],
        )?;
        conn.execute(
            "UPDATE dependencies SET is_indexed = 0 WHERE id = ?1",
            params![dep_id],
        )?;
        Ok(())
    }

    // === Scope Methods ===

    /// Adds scopes to the database
    pub fn add_scopes(&self, scopes: Vec<Scope>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        for scope in scopes {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO scopes
                (id, file_path, parent_id, kind, name, start_offset, end_offset, start_line, end_line)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    scope.id,
                    scope.file_path,
                    scope.parent_id,
                    scope.kind.as_str(),
                    scope.name,
                    scope.start_offset,
                    scope.end_offset,
                    scope.start_line,
                    scope.end_line,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Gets all scopes for a file
    pub fn get_file_scopes(&self, file_path: &str) -> Result<Vec<Scope>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, file_path, parent_id, kind, name, start_offset, end_offset, start_line, end_line
            FROM scopes
            WHERE file_path = ?1
            ORDER BY start_offset
            "#,
        )?;

        let scopes = stmt
            .query_map(params![file_path], |row| {
                let kind_str: String = row.get(3)?;
                Ok(Scope {
                    id: row.get(0)?,
                    file_path: row.get(1)?,
                    parent_id: row.get(2)?,
                    kind: ScopeKind::from_str(&kind_str).unwrap_or(ScopeKind::Block),
                    name: row.get(4)?,
                    start_offset: row.get(5)?,
                    end_offset: row.get(6)?,
                    start_line: row.get(7)?,
                    end_line: row.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(scopes)
    }

    /// Finds the scope containing a given offset in a file
    pub fn find_scope_at_offset(&self, file_path: &str, offset: u32) -> Result<Option<Scope>> {
        let conn = self.conn.lock().unwrap();
        let scope = conn
            .query_row(
                r#"
                SELECT id, file_path, parent_id, kind, name, start_offset, end_offset, start_line, end_line
                FROM scopes
                WHERE file_path = ?1 AND start_offset <= ?2 AND end_offset >= ?2
                ORDER BY (end_offset - start_offset) ASC
                LIMIT 1
                "#,
                params![file_path, offset],
                |row| {
                    let kind_str: String = row.get(3)?;
                    Ok(Scope {
                        id: row.get(0)?,
                        file_path: row.get(1)?,
                        parent_id: row.get(2)?,
                        kind: ScopeKind::from_str(&kind_str).unwrap_or(ScopeKind::Block),
                        name: row.get(4)?,
                        start_offset: row.get(5)?,
                        end_offset: row.get(6)?,
                        start_line: row.get(7)?,
                        end_line: row.get(8)?,
                    })
                },
            )
            .optional()?;

        Ok(scope)
    }

    /// Removes scopes for a file
    pub fn remove_file_scopes(&self, file_path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM scopes WHERE file_path = ?1", params![file_path])?;
        Ok(())
    }

    // === Call Edge Methods ===

    /// Adds call edges to the database
    pub fn add_call_edges(&self, edges: Vec<CallGraphEdge>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        for edge in edges {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO call_edges
                (caller_symbol_id, callee_symbol_id, callee_name, file_path, line, column_num, confidence, reason)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    edge.from,
                    edge.to,
                    edge.callee_name,
                    edge.call_site_file,
                    edge.call_site_line,
                    edge.call_site_column,
                    edge.confidence.as_str(),
                    edge.reason.map(|r| r.as_str().to_string()),
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Gets call edges from a caller
    pub fn get_call_edges_from(&self, caller_id: &str) -> Result<Vec<CallGraphEdge>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT caller_symbol_id, callee_symbol_id, callee_name, file_path, line, column_num, confidence, reason
            FROM call_edges
            WHERE caller_symbol_id = ?1
            ORDER BY line, column_num
            "#,
        )?;

        let edges = stmt
            .query_map(params![caller_id], |row| {
                let confidence_str: String = row.get(6)?;
                let reason_str: Option<String> = row.get(7)?;
                Ok(CallGraphEdge {
                    from: row.get(0)?,
                    to: row.get(1)?,
                    callee_name: row.get(2)?,
                    call_site_file: row.get(3)?,
                    call_site_line: row.get(4)?,
                    call_site_column: row.get(5)?,
                    confidence: CallConfidence::from_str(&confidence_str)
                        .unwrap_or(CallConfidence::Certain),
                    reason: reason_str.and_then(|s| UncertaintyReason::from_str(&s)),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(edges)
    }

    /// Gets call edges to a callee
    pub fn get_call_edges_to(&self, callee_id: &str) -> Result<Vec<CallGraphEdge>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT caller_symbol_id, callee_symbol_id, callee_name, file_path, line, column_num, confidence, reason
            FROM call_edges
            WHERE callee_symbol_id = ?1
            ORDER BY file_path, line
            "#,
        )?;

        let edges = stmt
            .query_map(params![callee_id], |row| {
                let confidence_str: String = row.get(6)?;
                let reason_str: Option<String> = row.get(7)?;
                Ok(CallGraphEdge {
                    from: row.get(0)?,
                    to: row.get(1)?,
                    callee_name: row.get(2)?,
                    call_site_file: row.get(3)?,
                    call_site_line: row.get(4)?,
                    call_site_column: row.get(5)?,
                    confidence: CallConfidence::from_str(&confidence_str)
                        .unwrap_or(CallConfidence::Certain),
                    reason: reason_str.and_then(|s| UncertaintyReason::from_str(&s)),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(edges)
    }

    /// Gets call edges by callee name (for unresolved lookups)
    pub fn get_call_edges_by_name(&self, callee_name: &str) -> Result<Vec<CallGraphEdge>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT caller_symbol_id, callee_symbol_id, callee_name, file_path, line, column_num, confidence, reason
            FROM call_edges
            WHERE callee_name = ?1
            ORDER BY file_path, line
            "#,
        )?;

        let edges = stmt
            .query_map(params![callee_name], |row| {
                let confidence_str: String = row.get(6)?;
                let reason_str: Option<String> = row.get(7)?;
                Ok(CallGraphEdge {
                    from: row.get(0)?,
                    to: row.get(1)?,
                    callee_name: row.get(2)?,
                    call_site_file: row.get(3)?,
                    call_site_line: row.get(4)?,
                    call_site_column: row.get(5)?,
                    confidence: CallConfidence::from_str(&confidence_str)
                        .unwrap_or(CallConfidence::Certain),
                    reason: reason_str.and_then(|s| UncertaintyReason::from_str(&s)),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(edges)
    }

    /// Removes call edges for a file
    pub fn remove_file_call_edges(&self, file_path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM call_edges WHERE file_path = ?1",
            params![file_path],
        )?;
        Ok(())
    }

    // === Symbol Metrics Methods ===

    /// Updates metrics for a symbol
    pub fn update_symbol_metrics(&self, metrics: &SymbolMetrics) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO symbol_metrics
            (symbol_id, pagerank, incoming_refs, outgoing_refs, git_recency)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                metrics.symbol_id,
                metrics.pagerank,
                metrics.incoming_refs,
                metrics.outgoing_refs,
                metrics.git_recency,
            ],
        )?;
        Ok(())
    }

    /// Gets metrics for a symbol
    pub fn get_symbol_metrics(&self, symbol_id: &str) -> Result<Option<SymbolMetrics>> {
        let conn = self.conn.lock().unwrap();
        let metrics = conn
            .query_row(
                r#"
                SELECT symbol_id, pagerank, incoming_refs, outgoing_refs, git_recency
                FROM symbol_metrics
                WHERE symbol_id = ?1
                "#,
                params![symbol_id],
                |row| {
                    Ok(SymbolMetrics {
                        symbol_id: row.get(0)?,
                        pagerank: row.get(1)?,
                        incoming_refs: row.get(2)?,
                        outgoing_refs: row.get(3)?,
                        git_recency: row.get(4)?,
                    })
                },
            )
            .optional()?;

        Ok(metrics)
    }

    /// Gets metrics for multiple symbols in a single query.
    /// Returns a HashMap from symbol_id to SymbolMetrics.
    /// This avoids N+1 queries when computing advanced scores for search results.
    pub fn get_symbol_metrics_batch(
        &self,
        symbol_ids: &[&str],
    ) -> Result<std::collections::HashMap<String, SymbolMetrics>> {
        use std::collections::HashMap;

        if symbol_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let conn = self.conn.lock().unwrap();

        // Build query with placeholders
        let placeholders: Vec<String> = symbol_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            r#"
            SELECT symbol_id, pagerank, incoming_refs, outgoing_refs, git_recency
            FROM symbol_metrics
            WHERE symbol_id IN ({})
            "#,
            placeholders.join(",")
        );

        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            symbol_ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();

        let rows = stmt.query_map(params.as_slice(), |row| {
            Ok(SymbolMetrics {
                symbol_id: row.get(0)?,
                pagerank: row.get(1)?,
                incoming_refs: row.get(2)?,
                outgoing_refs: row.get(3)?,
                git_recency: row.get(4)?,
            })
        })?;

        let mut result = HashMap::with_capacity(symbol_ids.len());
        for metrics in rows.flatten() {
            result.insert(metrics.symbol_id.clone(), metrics);
        }

        Ok(result)
    }

    /// Batch updates symbol metrics
    pub fn update_symbol_metrics_batch(&self, metrics_list: Vec<SymbolMetrics>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        for metrics in metrics_list {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO symbol_metrics
                (symbol_id, pagerank, incoming_refs, outgoing_refs, git_recency)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                params![
                    metrics.symbol_id,
                    metrics.pagerank,
                    metrics.incoming_refs,
                    metrics.outgoing_refs,
                    metrics.git_recency,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Batch insert all extraction results (symbols, references, imports) in a single transaction.
    /// Returns the total number of symbols inserted.
    pub fn add_extraction_results_batch(&self, results: Vec<ExtractionResult>) -> Result<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let mut total_symbols = 0;

        for result in results {
            // Insert symbols
            for symbol in result.symbols {
                tx.execute(
                    r#"
                    INSERT OR REPLACE INTO symbols
                    (id, name, kind, file_path, start_line, start_column, end_line, end_column,
                     language, visibility, signature, doc_comment, parent)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                    "#,
                    params![
                        symbol.id,
                        symbol.name,
                        symbol.kind.as_str(),
                        symbol.location.file_path,
                        symbol.location.start_line,
                        symbol.location.start_column,
                        symbol.location.end_line,
                        symbol.location.end_column,
                        symbol.language,
                        symbol.visibility.as_ref().map(|v| v.as_str()),
                        symbol.signature,
                        symbol.doc_comment,
                        symbol.parent,
                    ],
                )?;
                total_symbols += 1;
            }

            // Insert references
            for reference in result.references {
                tx.execute(
                    r#"
                    INSERT OR REPLACE INTO symbol_references
                    (symbol_id, symbol_name, referenced_in_file, line, column_num, reference_kind)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    "#,
                    params![
                        reference.symbol_id,
                        reference.symbol_name,
                        reference.file_path,
                        reference.line,
                        reference.column,
                        reference.kind.as_str(),
                    ],
                )?;
            }

            // Insert imports
            for import in result.imports {
                tx.execute(
                    r#"
                    INSERT OR REPLACE INTO file_imports
                    (file_path, imported_path, imported_symbol, import_type)
                    VALUES (?1, ?2, ?3, ?4)
                    "#,
                    params![
                        import.file_path,
                        import.imported_path,
                        import.imported_symbol,
                        import.import_type.as_str(),
                    ],
                )?;
            }
        }

        // Increment db revision in same transaction
        tx.execute(
            "UPDATE meta SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT) WHERE key = 'db_revision'",
            [],
        )?;

        tx.commit()?;
        Ok(total_symbols)
    }

    fn symbol_from_row(row: &rusqlite::Row) -> rusqlite::Result<Symbol> {
        let kind_str: String = row.get(2)?;
        let visibility_str: Option<String> = row.get(9)?;

        Ok(Symbol {
            id: row.get(0)?,
            name: row.get(1)?,
            kind: SymbolKind::from_str(&kind_str).unwrap_or(SymbolKind::Function),
            location: crate::index::Location {
                file_path: row.get(3)?,
                start_line: row.get(4)?,
                start_column: row.get(5)?,
                end_line: row.get(6)?,
                end_column: row.get(7)?,
            },
            language: row.get(8)?,
            visibility: visibility_str.and_then(|s| Visibility::from_str(&s)),
            signature: row.get(10)?,
            doc_comment: row.get(11)?,
            parent: row.get(12)?,
            scope_id: None, // Not loaded from basic queries
            fqdn: None,     // Not loaded from basic queries
            // P2 fields - not stored in DB yet, need migration to enable
            generic_params: Vec::new(),
            params: Vec::new(),
            return_type: None,
        })
    }

    /// Read symbol with extended fields including scope_id and fqdn
    #[allow(dead_code)]
    fn symbol_from_row_extended(row: &rusqlite::Row) -> rusqlite::Result<Symbol> {
        let kind_str: String = row.get(2)?;
        let visibility_str: Option<String> = row.get(9)?;

        Ok(Symbol {
            id: row.get(0)?,
            name: row.get(1)?,
            kind: SymbolKind::from_str(&kind_str).unwrap_or(SymbolKind::Function),
            location: crate::index::Location {
                file_path: row.get(3)?,
                start_line: row.get(4)?,
                start_column: row.get(5)?,
                end_line: row.get(6)?,
                end_column: row.get(7)?,
            },
            language: row.get(8)?,
            visibility: visibility_str.and_then(|s| Visibility::from_str(&s)),
            signature: row.get(10)?,
            doc_comment: row.get(11)?,
            parent: row.get(12)?,
            scope_id: row.get(13).ok(),
            fqdn: row.get(14).ok(),
            // P2 fields - not stored in DB yet, need migration to enable
            generic_params: Vec::new(),
            params: Vec::new(),
            return_type: None,
        })
    }

    /// Computes an advanced score for a symbol in search results.
    /// Scoring formula:
    /// - 0.4 * name_match_score (how well the name matches the query)
    /// - 0.2 * visibility_score (public > internal > private)
    /// - 0.2 * pagerank_score (importance based on call graph)
    /// - 0.1 * git_recency_score (recently modified symbols rank higher)
    /// - 0.1 * locality_score (symbols in same/nearby files rank higher)
    fn compute_advanced_score(
        &self,
        symbol: &Symbol,
        name_match_score: f64,
        current_file: Option<&str>,
    ) -> f64 {
        // Base score from name matching
        let name_score = name_match_score * 0.4;

        // Visibility score (public symbols are more likely to be searched for)
        let visibility_score = match symbol.visibility.as_ref() {
            Some(Visibility::Public) => 1.0,
            Some(Visibility::Internal) => 0.7,
            Some(Visibility::Protected) => 0.5,
            Some(Visibility::Private) => 0.3,
            None => 0.5, // Unknown visibility
        } * 0.2;

        // Try to get metrics for pagerank and recency
        let (pagerank_score, recency_score) =
            if let Ok(Some(metrics)) = self.get_symbol_metrics(&symbol.id) {
                // Normalize pagerank (typically 0-1, but cap at 1)
                let pr = (metrics.pagerank.min(1.0).max(0.0)) * 0.2;
                // Git recency is already 0-1
                let rec = (metrics.git_recency.min(1.0).max(0.0)) * 0.1;
                (pr, rec)
            } else {
                (0.0, 0.0)
            };

        // Locality score (same file = 1.0, same directory = 0.5, else 0.0)
        let locality_score = if let Some(current) = current_file {
            if symbol.location.file_path == current {
                1.0
            } else {
                // Check if in same directory
                let current_dir = std::path::Path::new(current).parent();
                let symbol_dir = std::path::Path::new(&symbol.location.file_path).parent();
                if current_dir.is_some() && current_dir == symbol_dir {
                    0.5
                } else {
                    0.0
                }
            }
        } else {
            0.0
        } * 0.1;

        name_score + visibility_score + pagerank_score + recency_score + locality_score
    }

    /// Computes an advanced score using pre-loaded metrics (avoids N+1 queries).
    /// Same scoring formula as compute_advanced_score but takes metrics as parameter.
    fn compute_advanced_score_with_metrics(
        &self,
        symbol: &Symbol,
        name_match_score: f64,
        current_file: Option<&str>,
        metrics: Option<&SymbolMetrics>,
    ) -> f64 {
        // Base score from name matching
        let name_score = name_match_score * 0.4;

        // Visibility score (public symbols are more likely to be searched for)
        let visibility_score = match symbol.visibility.as_ref() {
            Some(Visibility::Public) => 1.0,
            Some(Visibility::Internal) => 0.7,
            Some(Visibility::Protected) => 0.5,
            Some(Visibility::Private) => 0.3,
            None => 0.5, // Unknown visibility
        } * 0.2;

        // Use pre-loaded metrics for pagerank and recency
        let (pagerank_score, recency_score) = if let Some(m) = metrics {
            // Normalize pagerank (typically 0-1, but cap at 1)
            let pr = (m.pagerank.min(1.0).max(0.0)) * 0.2;
            // Git recency is already 0-1
            let rec = (m.git_recency.min(1.0).max(0.0)) * 0.1;
            (pr, rec)
        } else {
            (0.0, 0.0)
        };

        // Locality score (same file = 1.0, same directory = 0.5, else 0.0)
        let locality_score = if let Some(current) = current_file {
            if symbol.location.file_path == current {
                1.0
            } else {
                // Check if in same directory
                let current_dir = std::path::Path::new(current).parent();
                let symbol_dir = std::path::Path::new(&symbol.location.file_path).parent();
                if current_dir.is_some() && current_dir == symbol_dir {
                    0.5
                } else {
                    0.0
                }
            }
        } else {
            0.0
        } * 0.1;

        name_score + visibility_score + pagerank_score + recency_score + locality_score
    }

    /// Performs prefix search for short queries (less than 4 characters).
    /// Short queries are too ambiguous for fuzzy matching, so we only do prefix match.
    fn search_prefix(&self, query: &str, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        let limit = options.limit.unwrap_or(100);
        let query_lower = query.to_lowercase();
        let use_advanced = options.use_advanced_ranking.unwrap_or(false);
        let current_file = options.current_file.clone();

        // Fetch symbols from database
        let symbols = {
            let conn = self.conn.lock().unwrap();

            let mut sql = String::from(
                r#"
                SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                       language, visibility, signature, doc_comment, parent
                FROM symbols
                WHERE name LIKE ?1
                "#,
            );

            let prefix_pattern = format!("{}%", query.replace(['%', '_'], ""));
            let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(prefix_pattern)];

            if let Some(ref kinds) = options.kind_filter {
                let placeholders: Vec<String> = kinds.iter().map(|_| "?".to_string()).collect();
                sql.push_str(&format!(" AND kind IN ({})", placeholders.join(",")));
                for kind in kinds {
                    params_vec.push(Box::new(kind.as_str().to_string()));
                }
            }

            if let Some(ref langs) = options.language_filter {
                let placeholders: Vec<String> = langs.iter().map(|_| "?".to_string()).collect();
                sql.push_str(&format!(" AND language IN ({})", placeholders.join(",")));
                for lang in langs {
                    params_vec.push(Box::new(lang.clone()));
                }
            }

            if let Some(ref file) = options.file_filter {
                sql.push_str(" AND file_path LIKE ?");
                params_vec.push(Box::new(format!("%{}%", file)));
            }

            sql.push_str(&format!(" ORDER BY name LIMIT {}", limit));

            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params_vec.iter().map(|p| p.as_ref()).collect();

            let mut stmt = conn.prepare(&sql)?;
            let symbols: Vec<Symbol> = stmt
                .query_map(params_refs.as_slice(), Self::symbol_from_row)?
                .filter_map(|r| r.ok())
                .collect();
            symbols
        };

        // Calculate scores (outside of connection lock)
        let current_file_ref = current_file.as_deref();

        // First pass: calculate name scores
        let preliminary: Vec<(Symbol, f64)> = symbols
            .into_iter()
            .map(|symbol| {
                let name_lower = symbol.name.to_lowercase();
                // Exact match = 1.0, prefix match = 0.8, contains = 0.5
                let name_score = if name_lower == query_lower {
                    1.0
                } else if name_lower.starts_with(&query_lower) {
                    0.8
                } else {
                    0.5
                };
                (symbol, name_score)
            })
            .collect();

        // OPTIMIZATION: Batch load metrics for advanced ranking (avoids N+1 queries)
        let mut results: Vec<SearchResult> = if use_advanced && !preliminary.is_empty() {
            let symbol_ids: Vec<&str> = preliminary.iter().map(|(s, _)| s.id.as_str()).collect();
            let metrics_map = self.get_symbol_metrics_batch(&symbol_ids).unwrap_or_default();

            preliminary
                .into_iter()
                .map(|(symbol, name_score)| {
                    let score = self.compute_advanced_score_with_metrics(
                        &symbol,
                        name_score,
                        current_file_ref,
                        metrics_map.get(&symbol.id),
                    );
                    SearchResult { symbol, score }
                })
                .collect()
        } else {
            preliminary
                .into_iter()
                .map(|(symbol, name_score)| SearchResult {
                    symbol,
                    score: name_score,
                })
                .collect()
        };

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);

        Ok(results)
    }

    // === Summary-First Contract Methods ===

    /// Search with cursor-based pagination for deterministic results
    ///
    /// Uses sorting: (score DESC, kind, file_path, start_line, stable_id)
    /// Returns results after the cursor position.
    pub fn search_paginated(
        &self,
        query: &str,
        options: &SearchOptions,
        cursor: Option<&PaginationCursor>,
        include_total: bool,
    ) -> Result<(Vec<SearchResult>, Option<usize>)> {
        let limit = options.limit.unwrap_or(20);
        let conn = self.conn.lock().unwrap();
        let fts_query = format!("{}*", query.replace(['*', '"', '\''], ""));

        // Build base query
        let mut sql = String::from(
            r#"
            SELECT s.id, s.name, s.kind, s.file_path, s.start_line, s.start_column,
                   s.end_line, s.end_column, s.language, s.visibility, s.signature,
                   s.doc_comment, s.parent, s.stable_id,
                   bm25(symbols_fts) as score
            FROM symbols s
            JOIN symbols_fts ON s.rowid = symbols_fts.rowid
            WHERE symbols_fts MATCH ?1
            "#,
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(fts_query.clone())];

        // Apply filters
        if let Some(ref kinds) = options.kind_filter {
            let placeholders: Vec<String> = kinds.iter().map(|_| "?".to_string()).collect();
            sql.push_str(&format!(" AND s.kind IN ({})", placeholders.join(",")));
            for kind in kinds {
                params_vec.push(Box::new(kind.as_str().to_string()));
            }
        }

        if let Some(ref langs) = options.language_filter {
            let placeholders: Vec<String> = langs.iter().map(|_| "?".to_string()).collect();
            sql.push_str(&format!(" AND s.language IN ({})", placeholders.join(",")));
            for lang in langs {
                params_vec.push(Box::new(lang.clone()));
            }
        }

        if let Some(ref file) = options.file_filter {
            sql.push_str(" AND s.file_path LIKE ?");
            params_vec.push(Box::new(format!("%{}%", file)));
        }

        // Apply cursor for pagination (keyset pagination)
        if let Some(cur) = cursor {
            if let (Some(score), Some(kind), Some(file), Some(line)) =
                (&cur.score, &cur.kind, &cur.file, &cur.line)
            {
                sql.push_str(
                    r#" AND (
                    -bm25(symbols_fts) < ?
                    OR (-bm25(symbols_fts) = ? AND s.kind > ?)
                    OR (-bm25(symbols_fts) = ? AND s.kind = ? AND s.file_path > ?)
                    OR (-bm25(symbols_fts) = ? AND s.kind = ? AND s.file_path = ? AND s.start_line > ?)
                )"#,
                );
                // score (inverted for bm25)
                params_vec.push(Box::new(*score));
                params_vec.push(Box::new(*score));
                params_vec.push(Box::new(kind.clone()));
                params_vec.push(Box::new(*score));
                params_vec.push(Box::new(kind.clone()));
                params_vec.push(Box::new(file.clone()));
                params_vec.push(Box::new(*score));
                params_vec.push(Box::new(kind.clone()));
                params_vec.push(Box::new(file.clone()));
                params_vec.push(Box::new(*line));
            } else if let Some(offset) = cur.offset {
                // Fallback to offset-based pagination
                sql.push_str(&format!(" OFFSET {}", offset));
            }
        }

        // Deterministic ordering
        sql.push_str(
            " ORDER BY -bm25(symbols_fts) DESC, s.kind ASC, s.file_path ASC, s.start_line ASC, s.id ASC",
        );
        sql.push_str(&format!(" LIMIT {}", limit));

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let results: Vec<SearchResult> = stmt
            .query_map(params_refs.as_slice(), |row| {
                let symbol = Self::symbol_from_row(row)?;
                let score: f64 = row.get(14)?;
                Ok(SearchResult {
                    symbol,
                    score: -score,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Optionally count total
        let total = if include_total {
            let count_sql = format!(
                r#"
                SELECT COUNT(*)
                FROM symbols s
                JOIN symbols_fts ON s.rowid = symbols_fts.rowid
                WHERE symbols_fts MATCH ?1
                "#
            );
            let count: i64 = conn.query_row(&count_sql, params![fts_query], |row| row.get(0))?;
            Some(count as usize)
        } else {
            None
        };

        Ok((results, total))
    }

    /// Search excluding specific file paths (for overlay-priority search)
    pub fn search_excluding_files(
        &self,
        query: &str,
        options: &SearchOptions,
        exclude_files: &[String],
    ) -> Result<Vec<SearchResult>> {
        if exclude_files.is_empty() {
            return CodeIndex::search(self, query, options);
        }

        let limit = options.limit.unwrap_or(100);
        let conn = self.conn.lock().unwrap();
        let fts_query = format!("{}*", query.replace(['*', '"', '\''], ""));

        let mut sql = String::from(
            r#"
            SELECT s.id, s.name, s.kind, s.file_path, s.start_line, s.start_column,
                   s.end_line, s.end_column, s.language, s.visibility, s.signature,
                   s.doc_comment, s.parent,
                   bm25(symbols_fts) as score
            FROM symbols s
            JOIN symbols_fts ON s.rowid = symbols_fts.rowid
            WHERE symbols_fts MATCH ?1
            "#,
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(fts_query)];

        // Exclude files
        let placeholders: Vec<String> = exclude_files.iter().map(|_| "?".to_string()).collect();
        sql.push_str(&format!(
            " AND s.file_path NOT IN ({})",
            placeholders.join(",")
        ));
        for file in exclude_files {
            params_vec.push(Box::new(file.clone()));
        }

        // Apply other filters
        if let Some(ref kinds) = options.kind_filter {
            let placeholders: Vec<String> = kinds.iter().map(|_| "?".to_string()).collect();
            sql.push_str(&format!(" AND s.kind IN ({})", placeholders.join(",")));
            for kind in kinds {
                params_vec.push(Box::new(kind.as_str().to_string()));
            }
        }

        if let Some(ref langs) = options.language_filter {
            let placeholders: Vec<String> = langs.iter().map(|_| "?".to_string()).collect();
            sql.push_str(&format!(" AND s.language IN ({})", placeholders.join(",")));
            for lang in langs {
                params_vec.push(Box::new(lang.clone()));
            }
        }

        if let Some(ref file) = options.file_filter {
            sql.push_str(" AND s.file_path LIKE ?");
            params_vec.push(Box::new(format!("%{}%", file)));
        }

        sql.push_str(&format!(" ORDER BY score LIMIT {}", limit));

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let results: Vec<SearchResult> = stmt
            .query_map(params_refs.as_slice(), |row| {
                let symbol = Self::symbol_from_row(row)?;
                let score: f64 = row.get(13)?;
                Ok(SearchResult {
                    symbol,
                    score: -score,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }

    /// Get symbol by stable ID
    pub fn get_symbol_by_stable_id(&self, stable_id: &str) -> Result<Option<Symbol>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                   language, visibility, signature, doc_comment, parent
            FROM symbols WHERE stable_id = ?1
            "#,
        )?;

        let symbol = stmt
            .query_row(params![stable_id], Self::symbol_from_row)
            .optional()?;

        Ok(symbol)
    }

    /// Update stable_id for a symbol
    pub fn update_stable_id(&self, symbol_id: &str, stable_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE symbols SET stable_id = ?1 WHERE id = ?2",
            params![stable_id, symbol_id],
        )?;
        Ok(())
    }

    /// Finds the type of a receiver expression by looking up symbol definitions.
    /// Returns the parent type name if found.
    /// This is used for type-aware method resolution.
    pub fn infer_receiver_type(&self, receiver: &str, file_path: &str) -> Option<String> {
        // Check if receiver is a known symbol in the same file
        let conn = self.conn.lock().unwrap();

        // First, try to find a variable or field with this name in the same file
        let result: Option<String> = conn
            .query_row(
                r#"
                SELECT parent FROM symbols
                WHERE name = ?1 AND file_path = ?2
                      AND kind IN ('variable', 'field', 'constant')
                LIMIT 1
                "#,
                params![receiver, file_path],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        if result.is_some() {
            return result;
        }

        // Check if receiver is a type name itself (static method call)
        let type_exists: bool = conn
            .query_row(
                r#"
                SELECT 1 FROM symbols
                WHERE name = ?1 AND kind IN ('struct', 'class', 'interface', 'trait', 'enum')
                LIMIT 1
                "#,
                params![receiver],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if type_exists {
            return Some(receiver.to_string());
        }

        None
    }
}

impl CodeIndex for SqliteIndex {
    fn add_symbol(&self, symbol: Symbol) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO symbols
            (id, name, kind, file_path, start_line, start_column, end_line, end_column,
             language, visibility, signature, doc_comment, parent)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                symbol.id,
                symbol.name,
                symbol.kind.as_str(),
                symbol.location.file_path,
                symbol.location.start_line,
                symbol.location.start_column,
                symbol.location.end_line,
                symbol.location.end_column,
                symbol.language,
                symbol.visibility.as_ref().map(|v| v.as_str()),
                symbol.signature,
                symbol.doc_comment,
                symbol.parent,
            ],
        )?;
        Ok(())
    }

    fn add_symbols(&self, symbols: Vec<Symbol>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        for symbol in symbols {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO symbols
                (id, name, kind, file_path, start_line, start_column, end_line, end_column,
                 language, visibility, signature, doc_comment, parent)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                "#,
                params![
                    symbol.id,
                    symbol.name,
                    symbol.kind.as_str(),
                    symbol.location.file_path,
                    symbol.location.start_line,
                    symbol.location.start_column,
                    symbol.location.end_line,
                    symbol.location.end_column,
                    symbol.language,
                    symbol.visibility.as_ref().map(|v| v.as_str()),
                    symbol.signature,
                    symbol.doc_comment,
                    symbol.parent,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    fn remove_file(&self, file_path: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // 1. Get symbol IDs for this file (needed for cascading deletes)
        let symbol_ids: Vec<String> = {
            let mut stmt = tx.prepare("SELECT id FROM symbols WHERE file_path = ?1")?;
            let ids = stmt
                .query_map(params![file_path], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            ids
        };

        // 2. Delete data referencing these symbols
        for symbol_id in &symbol_ids {
            tx.execute(
                "DELETE FROM symbol_references WHERE symbol_id = ?1",
                params![symbol_id],
            )?;
            tx.execute(
                "DELETE FROM call_edges WHERE caller_symbol_id = ?1 OR callee_symbol_id = ?1",
                params![symbol_id],
            )?;
            tx.execute(
                "DELETE FROM symbol_metrics WHERE symbol_id = ?1",
                params![symbol_id],
            )?;
        }

        // 3. Delete data referencing this file path
        tx.execute(
            "DELETE FROM symbol_references WHERE referenced_in_file = ?1",
            params![file_path],
        )?;
        tx.execute(
            "DELETE FROM file_imports WHERE file_path = ?1",
            params![file_path],
        )?;
        tx.execute(
            "DELETE FROM scopes WHERE file_path = ?1",
            params![file_path],
        )?;
        tx.execute(
            "DELETE FROM call_edges WHERE file_path = ?1",
            params![file_path],
        )?;

        // 4. Delete symbols and file entry
        tx.execute(
            "DELETE FROM symbols WHERE file_path = ?1",
            params![file_path],
        )?;
        tx.execute("DELETE FROM files WHERE path = ?1", params![file_path])?;

        // 5. Increment db revision
        tx.execute(
            "UPDATE meta SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT) WHERE key = 'db_revision'",
            [],
        )?;

        tx.commit()?;
        Ok(())
    }

    fn get_symbol(&self, id: &str) -> Result<Option<Symbol>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                   language, visibility, signature, doc_comment, parent
            FROM symbols WHERE id = ?1
            "#,
        )?;

        let symbol = stmt
            .query_row(params![id], Self::symbol_from_row)
            .optional()?;

        Ok(symbol)
    }

    fn search(&self, query: &str, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        let limit = options.limit.unwrap_or(100);
        let use_advanced = options.use_advanced_ranking.unwrap_or(false);
        let current_file = options.current_file.clone();

        // Fetch results from database
        let initial_results = {
            let conn = self.conn.lock().unwrap();
            let fts_query = format!("{}*", query.replace(['*', '"', '\''], ""));

            let mut sql = String::from(
                r#"
                SELECT s.id, s.name, s.kind, s.file_path, s.start_line, s.start_column,
                       s.end_line, s.end_column, s.language, s.visibility, s.signature,
                       s.doc_comment, s.parent,
                       bm25(symbols_fts) as score
                FROM symbols s
                JOIN symbols_fts ON s.rowid = symbols_fts.rowid
                WHERE symbols_fts MATCH ?1
                "#,
            );

            let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(fts_query)];

            if let Some(ref kinds) = options.kind_filter {
                let placeholders: Vec<String> = kinds.iter().map(|_| "?".to_string()).collect();
                sql.push_str(&format!(" AND s.kind IN ({})", placeholders.join(",")));
                for kind in kinds {
                    params_vec.push(Box::new(kind.as_str().to_string()));
                }
            }

            if let Some(ref langs) = options.language_filter {
                let placeholders: Vec<String> = langs.iter().map(|_| "?".to_string()).collect();
                sql.push_str(&format!(" AND s.language IN ({})", placeholders.join(",")));
                for lang in langs {
                    params_vec.push(Box::new(lang.clone()));
                }
            }

            if let Some(ref file) = options.file_filter {
                sql.push_str(" AND s.file_path LIKE ?");
                params_vec.push(Box::new(format!("%{}%", file)));
            }

            // Get more results initially if we're using advanced ranking (need to re-sort)
            let fetch_limit = if use_advanced {
                limit * 3 // Fetch more to allow for re-ranking
            } else {
                limit
            };
            sql.push_str(&format!(" ORDER BY score LIMIT {}", fetch_limit));

            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params_vec.iter().map(|p| p.as_ref()).collect();

            let mut stmt = conn.prepare(&sql)?;
            let results: Vec<(Symbol, f64)> = stmt
                .query_map(params_refs.as_slice(), |row| {
                    let symbol = Self::symbol_from_row(row)?;
                    let score: f64 = row.get(13)?;
                    Ok((symbol, -score)) // bm25 returns negative scores
                })?
                .filter_map(|r| r.ok())
                .collect();
            results
        };

        // Apply advanced ranking if enabled (outside of connection lock)
        let current_file_ref = current_file.as_deref();

        let mut results: Vec<SearchResult> = if use_advanced {
            initial_results
                .into_iter()
                .map(|(symbol, bm25_score)| {
                    // Normalize bm25 score to 0-1 range (approximation)
                    let name_match = (bm25_score / 10.0).min(1.0).max(0.0);
                    let final_score =
                        self.compute_advanced_score(&symbol, name_match, current_file_ref);
                    SearchResult {
                        symbol,
                        score: final_score,
                    }
                })
                .collect()
        } else {
            initial_results
                .into_iter()
                .map(|(symbol, score)| SearchResult { symbol, score })
                .collect()
        };

        // Re-sort and truncate if using advanced ranking
        if use_advanced {
            results.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            results.truncate(limit);
        }

        Ok(results)
    }

    fn search_fuzzy(&self, query: &str, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        use strsim::jaro_winkler;

        // For short queries (< 4 chars), fuzzy search is too ambiguous.
        // Fall back to prefix search for better results.
        if query.len() < 4 {
            return self.search_prefix(query, options);
        }

        let limit = options.limit.unwrap_or(100);
        let threshold = options.fuzzy_threshold.unwrap_or(0.7);
        let use_advanced = options.use_advanced_ranking.unwrap_or(false);
        let current_file = options.current_file.clone();
        let query_lower = query.to_lowercase();

        // OPTIMIZATION 1: Pre-filter candidates using SQL LIKE on first few characters
        // This reduces the number of symbols loaded from O(n) to O(k) where k << n
        // We use multiple LIKE patterns to catch typos in first characters:
        // - Exact prefix match (e.g., "func%" for "function")
        // - First char wildcard (e.g., "_unc%" to catch "bunction" typo)
        // - Contains the query substring (fallback for rearranged chars)
        let prefix_len = std::cmp::min(3, query_lower.len());
        let prefix = &query_lower[..prefix_len];

        let candidates = {
            let conn = self.conn.lock().unwrap();

            let mut sql = String::from(
                r#"
                SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                       language, visibility, signature, doc_comment, parent
                FROM symbols
                WHERE (
                    LOWER(name) LIKE ? OR
                    LOWER(name) LIKE ? OR
                    LOWER(name) LIKE ?
                )
                "#,
            );

            // Patterns: prefix%, _prefix% (skip first char), %query% (contains)
            let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(format!("{}%", prefix)),
                Box::new(format!("_{}%", &prefix[..std::cmp::min(2, prefix.len())])),
                Box::new(format!("%{}%", &query_lower[..std::cmp::min(4, query_lower.len())])),
            ];

            if let Some(ref kinds) = options.kind_filter {
                let placeholders: Vec<String> = kinds.iter().map(|_| "?".to_string()).collect();
                sql.push_str(&format!(" AND kind IN ({})", placeholders.join(",")));
                for kind in kinds {
                    params_vec.push(Box::new(kind.as_str().to_string()));
                }
            }

            if let Some(ref langs) = options.language_filter {
                let placeholders: Vec<String> = langs.iter().map(|_| "?".to_string()).collect();
                sql.push_str(&format!(" AND language IN ({})", placeholders.join(",")));
                for lang in langs {
                    params_vec.push(Box::new(lang.clone()));
                }
            }

            if let Some(ref file) = options.file_filter {
                sql.push_str(" AND file_path LIKE ?");
                params_vec.push(Box::new(format!("%{}%", file)));
            }

            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params_vec.iter().map(|p| p.as_ref()).collect();

            let mut stmt = conn.prepare(&sql)?;
            let symbols: Vec<Symbol> = stmt
                .query_map(params_refs.as_slice(), Self::symbol_from_row)?
                .filter_map(|r| r.ok())
                .collect();
            symbols
        };

        // First pass: calculate fuzzy scores and filter by threshold
        let current_file_ref = current_file.as_deref();
        let preliminary: Vec<(Symbol, f64)> = candidates
            .into_iter()
            .filter_map(|symbol| {
                let name_lower = symbol.name.to_lowercase();
                let fuzzy_score = jaro_winkler(&query_lower, &name_lower);

                if fuzzy_score >= threshold {
                    Some((symbol, fuzzy_score))
                } else {
                    None
                }
            })
            .collect();

        // OPTIMIZATION 2: Batch load metrics for advanced ranking (avoids N+1 queries)
        let scored: Vec<SearchResult> = if use_advanced && !preliminary.is_empty() {
            // Collect all symbol IDs
            let symbol_ids: Vec<&str> = preliminary.iter().map(|(s, _)| s.id.as_str()).collect();

            // Load all metrics in one query
            let metrics_map = self.get_symbol_metrics_batch(&symbol_ids).unwrap_or_default();

            // Calculate final scores with pre-loaded metrics
            preliminary
                .into_iter()
                .map(|(symbol, fuzzy_score)| {
                    let final_score = self.compute_advanced_score_with_metrics(
                        &symbol,
                        fuzzy_score,
                        current_file_ref,
                        metrics_map.get(&symbol.id),
                    );
                    SearchResult {
                        symbol,
                        score: final_score,
                    }
                })
                .collect()
        } else {
            // Simple scoring without advanced ranking
            preliminary
                .into_iter()
                .map(|(symbol, fuzzy_score)| SearchResult {
                    symbol,
                    score: fuzzy_score,
                })
                .collect()
        };

        // Sort by score descending and limit
        let mut scored = scored;
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored)
    }

    fn find_definition(&self, name: &str) -> Result<Vec<Symbol>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                   language, visibility, signature, doc_comment, parent
            FROM symbols
            WHERE name = ?1 AND kind NOT IN ('import', 'variable')
            ORDER BY file_path, start_line
            "#,
        )?;

        let symbols = stmt
            .query_map(params![name], Self::symbol_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(symbols)
    }

    /// Finds symbol definitions by name with optional parent type filter.
    /// This enables type-aware resolution: foo.bar() can be resolved to Type::bar
    /// instead of finding all methods named "bar".
    ///
    /// # Arguments
    /// * `name` - Symbol name to find
    /// * `parent_type` - Optional parent type name (e.g., "MyStruct" for methods)
    /// * `language` - Optional language filter
    fn find_definition_by_parent(
        &self,
        name: &str,
        parent_type: Option<&str>,
        language: Option<&str>,
    ) -> Result<Vec<Symbol>> {
        let conn = self.conn.lock().unwrap();

        let (sql, params): (String, Vec<Box<dyn rusqlite::ToSql>>) = match (parent_type, language) {
            (Some(parent), Some(lang)) => (
                r#"
                SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                       language, visibility, signature, doc_comment, parent
                FROM symbols
                WHERE name = ?1 AND parent = ?2 AND language = ?3
                      AND kind NOT IN ('import', 'variable')
                ORDER BY file_path, start_line
                "#
                .to_string(),
                vec![
                    Box::new(name.to_string()),
                    Box::new(parent.to_string()),
                    Box::new(lang.to_string()),
                ],
            ),
            (Some(parent), None) => (
                r#"
                SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                       language, visibility, signature, doc_comment, parent
                FROM symbols
                WHERE name = ?1 AND parent = ?2 AND kind NOT IN ('import', 'variable')
                ORDER BY file_path, start_line
                "#
                .to_string(),
                vec![
                    Box::new(name.to_string()),
                    Box::new(parent.to_string()),
                ],
            ),
            (None, Some(lang)) => (
                r#"
                SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                       language, visibility, signature, doc_comment, parent
                FROM symbols
                WHERE name = ?1 AND language = ?2 AND kind NOT IN ('import', 'variable')
                ORDER BY file_path, start_line
                "#
                .to_string(),
                vec![
                    Box::new(name.to_string()),
                    Box::new(lang.to_string()),
                ],
            ),
            (None, None) => (
                r#"
                SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                       language, visibility, signature, doc_comment, parent
                FROM symbols
                WHERE name = ?1 AND kind NOT IN ('import', 'variable')
                ORDER BY file_path, start_line
                "#
                .to_string(),
                vec![Box::new(name.to_string())],
            ),
        };

        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let symbols = stmt
            .query_map(params_refs.as_slice(), Self::symbol_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(symbols)
    }

    fn list_functions(&self, options: &SearchOptions) -> Result<Vec<Symbol>> {
        let conn = self.conn.lock().unwrap();
        let limit = options.limit.unwrap_or(1000);

        let mut sql = String::from(
            r#"
            SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                   language, visibility, signature, doc_comment, parent
            FROM symbols
            WHERE kind IN ('function', 'method')
            "#,
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(ref langs) = options.language_filter {
            let placeholders: Vec<String> = langs.iter().map(|_| "?".to_string()).collect();
            sql.push_str(&format!(" AND language IN ({})", placeholders.join(",")));
            for lang in langs {
                params_vec.push(Box::new(lang.clone()));
            }
        }

        if let Some(ref file) = options.file_filter {
            sql.push_str(" AND file_path LIKE ?");
            params_vec.push(Box::new(format!("%{}%", file)));
        }

        if let Some(ref pattern) = options.name_filter {
            // Convert glob pattern to SQL LIKE: * → %, ? → _
            let sql_pattern = pattern.replace('*', "%").replace('?', "_");
            sql.push_str(" AND name LIKE ?");
            params_vec.push(Box::new(sql_pattern));
        }

        sql.push_str(&format!(" ORDER BY file_path, start_line LIMIT {}", limit));

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let symbols = stmt
            .query_map(params_refs.as_slice(), Self::symbol_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(symbols)
    }

    fn list_types(&self, options: &SearchOptions) -> Result<Vec<Symbol>> {
        let conn = self.conn.lock().unwrap();
        let limit = options.limit.unwrap_or(1000);

        let mut sql = String::from(
            r#"
            SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                   language, visibility, signature, doc_comment, parent
            FROM symbols
            WHERE kind IN ('struct', 'class', 'interface', 'trait', 'enum', 'type_alias')
            "#,
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(ref langs) = options.language_filter {
            let placeholders: Vec<String> = langs.iter().map(|_| "?".to_string()).collect();
            sql.push_str(&format!(" AND language IN ({})", placeholders.join(",")));
            for lang in langs {
                params_vec.push(Box::new(lang.clone()));
            }
        }

        if let Some(ref file) = options.file_filter {
            sql.push_str(" AND file_path LIKE ?");
            params_vec.push(Box::new(format!("%{}%", file)));
        }

        if let Some(ref pattern) = options.name_filter {
            // Convert glob pattern to SQL LIKE: * → %, ? → _
            let sql_pattern = pattern.replace('*', "%").replace('?', "_");
            sql.push_str(" AND name LIKE ?");
            params_vec.push(Box::new(sql_pattern));
        }

        sql.push_str(&format!(" ORDER BY file_path, start_line LIMIT {}", limit));

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let symbols = stmt
            .query_map(params_refs.as_slice(), Self::symbol_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(symbols)
    }

    fn get_file_symbols(&self, file_path: &str) -> Result<Vec<Symbol>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                   language, visibility, signature, doc_comment, parent
            FROM symbols
            WHERE file_path = ?1
            ORDER BY start_line, start_column
            "#,
        )?;

        let symbols = stmt
            .query_map(params![file_path], Self::symbol_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(symbols)
    }

    fn get_stats(&self) -> Result<IndexStats> {
        let conn = self.conn.lock().unwrap();

        let total_files: i64 =
            conn.query_row("SELECT COUNT(DISTINCT file_path) FROM symbols", [], |row| {
                row.get(0)
            })?;

        let total_symbols: i64 =
            conn.query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;

        let mut stmt = conn.prepare("SELECT kind, COUNT(*) FROM symbols GROUP BY kind")?;
        let symbols_by_kind: Vec<(String, usize)> = stmt
            .query_map([], |row| {
                let count: i64 = row.get(1)?;
                Ok((row.get(0)?, count as usize))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut stmt = conn.prepare("SELECT language, COUNT(*) FROM symbols GROUP BY language")?;
        let symbols_by_language: Vec<(String, usize)> = stmt
            .query_map([], |row| {
                let count: i64 = row.get(1)?;
                Ok((row.get(0)?, count as usize))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut stmt = conn.prepare(
            "SELECT language, COUNT(DISTINCT file_path) FROM symbols GROUP BY language",
        )?;
        let files_by_language: Vec<(String, usize)> = stmt
            .query_map([], |row| {
                let count: i64 = row.get(1)?;
                Ok((row.get(0)?, count as usize))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(IndexStats {
            total_files: total_files as usize,
            total_symbols: total_symbols as usize,
            symbols_by_kind,
            symbols_by_language,
            files_by_language,
        })
    }

    fn clear(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM symbols", [])?;
        conn.execute("DELETE FROM files", [])?;
        conn.execute("DELETE FROM symbols_fts", [])?;
        conn.execute("DELETE FROM symbol_references", [])?;
        conn.execute("DELETE FROM file_imports", [])?;
        Ok(())
    }

    fn add_references(&self, references: Vec<SymbolReference>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        for reference in references {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO symbol_references
                (symbol_id, symbol_name, referenced_in_file, line, column_num, reference_kind)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    reference.symbol_id,
                    reference.symbol_name,
                    reference.file_path,
                    reference.line,
                    reference.column,
                    reference.kind.as_str(),
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    fn find_references(
        &self,
        symbol_name: &str,
        options: &SearchOptions,
    ) -> Result<Vec<SymbolReference>> {
        let conn = self.conn.lock().unwrap();
        let limit = options.limit.unwrap_or(100);

        let mut sql = String::from(
            r#"
            SELECT symbol_id, symbol_name, referenced_in_file, line, column_num, reference_kind
            FROM symbol_references
            WHERE symbol_name = ?1
            "#,
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(symbol_name.to_string())];

        if let Some(ref file) = options.file_filter {
            sql.push_str(" AND referenced_in_file LIKE ?");
            params_vec.push(Box::new(format!("%{}%", file)));
        }

        sql.push_str(&format!(" ORDER BY referenced_in_file, line LIMIT {}", limit));

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let refs = stmt
            .query_map(params_refs.as_slice(), |row| {
                let kind_str: String = row.get(5)?;
                Ok(SymbolReference {
                    symbol_id: row.get(0)?,
                    symbol_name: row.get(1)?,
                    file_path: row.get(2)?,
                    line: row.get(3)?,
                    column: row.get(4)?,
                    kind: ReferenceKind::from_str(&kind_str).unwrap_or(ReferenceKind::Call),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(refs)
    }

    fn find_callers(&self, function_name: &str, depth: Option<u32>) -> Result<Vec<SymbolReference>> {
        let conn = self.conn.lock().unwrap();
        let max_depth = depth.unwrap_or(1);

        // For depth > 1, we would need recursive queries
        // For now, implement single-level caller search
        let sql = r#"
            SELECT symbol_id, symbol_name, referenced_in_file, line, column_num, reference_kind
            FROM symbol_references
            WHERE symbol_name = ?1 AND reference_kind = 'call'
            ORDER BY referenced_in_file, line
            LIMIT 100
        "#;

        let mut stmt = conn.prepare(sql)?;
        let refs = stmt
            .query_map(params![function_name], |row| {
                let kind_str: String = row.get(5)?;
                Ok(SymbolReference {
                    symbol_id: row.get(0)?,
                    symbol_name: row.get(1)?,
                    file_path: row.get(2)?,
                    line: row.get(3)?,
                    column: row.get(4)?,
                    kind: ReferenceKind::from_str(&kind_str).unwrap_or(ReferenceKind::Call),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        // If depth > 1, recursively find callers of the functions that call this one
        if max_depth > 1 && !refs.is_empty() {
            // TODO: Implement recursive caller search
            // For now, just return direct callers
        }

        Ok(refs)
    }

    fn find_implementations(&self, trait_name: &str) -> Result<Vec<Symbol>> {
        let conn = self.conn.lock().unwrap();

        // Find symbols that extend/implement the given trait/interface
        let sql = r#"
            SELECT DISTINCT s.id, s.name, s.kind, s.file_path, s.start_line, s.start_column,
                   s.end_line, s.end_column, s.language, s.visibility, s.signature,
                   s.doc_comment, s.parent
            FROM symbols s
            JOIN symbol_references r ON s.name = (
                SELECT DISTINCT
                    CASE
                        WHEN instr(referenced_in_file, '/') > 0
                        THEN substr(referenced_in_file, instr(referenced_in_file, '/') + 1)
                        ELSE referenced_in_file
                    END
                FROM symbol_references
                WHERE symbol_name = ?1 AND reference_kind = 'extend'
            )
            WHERE s.kind IN ('struct', 'class', 'enum')

            UNION

            SELECT s.id, s.name, s.kind, s.file_path, s.start_line, s.start_column,
                   s.end_line, s.end_column, s.language, s.visibility, s.signature,
                   s.doc_comment, s.parent
            FROM symbols s
            WHERE s.id IN (
                SELECT DISTINCT symbol_id FROM symbol_references
                WHERE symbol_name = ?1 AND reference_kind = 'extend' AND symbol_id IS NOT NULL
            )
        "#;

        let mut stmt = conn.prepare(sql)?;
        let symbols = stmt
            .query_map(params![trait_name], Self::symbol_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(symbols)
    }

    fn get_symbol_members(&self, type_name: &str) -> Result<Vec<Symbol>> {
        let conn = self.conn.lock().unwrap();

        // Find all methods and fields that have this type as parent
        let sql = r#"
            SELECT id, name, kind, file_path, start_line, start_column, end_line, end_column,
                   language, visibility, signature, doc_comment, parent
            FROM symbols
            WHERE parent = ?1 OR (
                file_path IN (SELECT file_path FROM symbols WHERE name = ?1)
                AND kind IN ('method', 'field')
                AND start_line > (SELECT start_line FROM symbols WHERE name = ?1 LIMIT 1)
                AND start_line < (SELECT end_line FROM symbols WHERE name = ?1 LIMIT 1)
            )
            ORDER BY start_line
        "#;

        let mut stmt = conn.prepare(sql)?;
        let symbols = stmt
            .query_map(params![type_name], Self::symbol_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(symbols)
    }

    fn add_imports(&self, imports: Vec<FileImport>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        for import in imports {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO file_imports
                (file_path, imported_path, imported_symbol, import_type)
                VALUES (?1, ?2, ?3, ?4)
                "#,
                params![
                    import.file_path,
                    import.imported_path,
                    import.imported_symbol,
                    import.import_type.as_str(),
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    fn get_file_imports(&self, file_path: &str) -> Result<Vec<FileImport>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT file_path, imported_path, imported_symbol, import_type
            FROM file_imports
            WHERE file_path = ?1
            ORDER BY imported_path, imported_symbol
            "#,
        )?;

        let imports = stmt
            .query_map(params![file_path], |row| {
                let import_type_str: String = row.get(3)?;
                Ok(FileImport {
                    file_path: row.get(0)?,
                    imported_path: row.get(1)?,
                    imported_symbol: row.get(2)?,
                    import_type: ImportType::from_str(&import_type_str).unwrap_or(ImportType::Symbol),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(imports)
    }

    fn get_file_importers(&self, file_path: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT DISTINCT file_path
            FROM file_imports
            WHERE imported_path LIKE ?1
            ORDER BY file_path
            "#,
        )?;

        let importers = stmt
            .query_map(params![format!("%{}", file_path)], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;

        Ok(importers)
    }

    fn find_callees(&self, function_name: &str) -> Result<Vec<SymbolReference>> {
        let conn = self.conn.lock().unwrap();

        // First find the function's file and line range
        let function_info: Option<(String, u32, u32)> = conn
            .query_row(
                r#"
                SELECT file_path, start_line, end_line
                FROM symbols
                WHERE name = ?1 AND kind IN ('function', 'method')
                LIMIT 1
                "#,
                params![function_name],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        if function_info.is_none() {
            return Ok(Vec::new());
        }

        let (file_path, start_line, end_line) = function_info.unwrap();

        // Find all call references within the function's line range
        let mut stmt = conn.prepare(
            r#"
            SELECT symbol_id, symbol_name, referenced_in_file, line, column_num, reference_kind
            FROM symbol_references
            WHERE referenced_in_file = ?1
              AND line >= ?2
              AND line <= ?3
              AND reference_kind = 'call'
            ORDER BY line, column_num
            "#,
        )?;

        let refs = stmt
            .query_map(params![file_path, start_line, end_line], |row| {
                let kind_str: String = row.get(5)?;
                Ok(SymbolReference {
                    symbol_id: row.get(0)?,
                    symbol_name: row.get(1)?,
                    file_path: row.get(2)?,
                    line: row.get(3)?,
                    column: row.get(4)?,
                    kind: ReferenceKind::from_str(&kind_str).unwrap_or(ReferenceKind::Call),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(refs)
    }

    fn get_call_graph(&self, entry_point: &str, max_depth: u32) -> Result<CallGraph> {
        use std::collections::{HashSet, VecDeque};

        let conn = self.conn.lock().unwrap();
        let mut graph = CallGraph::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, u32)> = VecDeque::new();

        // Find the entry point function
        let entry_info: Option<(String, String, u32)> = conn
            .query_row(
                r#"
                SELECT id, file_path, start_line
                FROM symbols
                WHERE name = ?1 AND kind IN ('function', 'method')
                LIMIT 1
                "#,
                params![entry_point],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        if entry_info.is_none() {
            return Ok(graph);
        }

        let (entry_id, entry_file, entry_line) = entry_info.unwrap();

        // Add entry point node
        graph.nodes.push(CallGraphNode {
            id: entry_id.clone(),
            name: entry_point.to_string(),
            file_path: entry_file.clone(),
            line: entry_line,
            depth: 0,
        });

        visited.insert(entry_point.to_string());
        queue.push_back((entry_point.to_string(), 0));

        // BFS traversal
        while let Some((func_name, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            // Find function's location
            let func_info: Option<(String, String, u32, u32)> = conn
                .query_row(
                    r#"
                    SELECT id, file_path, start_line, end_line
                    FROM symbols
                    WHERE name = ?1 AND kind IN ('function', 'method')
                    LIMIT 1
                    "#,
                    params![&func_name],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;

            if func_info.is_none() {
                continue;
            }

            let (from_id, file_path, start_line, end_line) = func_info.unwrap();

            // Find all calls within this function
            let mut stmt = conn.prepare(
                r#"
                SELECT DISTINCT sr.symbol_name, sr.line
                FROM symbol_references sr
                WHERE sr.referenced_in_file = ?1
                  AND sr.line >= ?2
                  AND sr.line <= ?3
                  AND sr.reference_kind = 'call'
                "#,
            )?;

            let callees: Vec<(String, u32)> = stmt
                .query_map(params![&file_path, start_line, end_line], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();

            for (callee_name, call_line) in callees {
                // Find the callee function
                let callee_info: Option<(String, String, u32)> = conn
                    .query_row(
                        r#"
                        SELECT id, file_path, start_line
                        FROM symbols
                        WHERE name = ?1 AND kind IN ('function', 'method')
                        LIMIT 1
                        "#,
                        params![&callee_name],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;

                if let Some((to_id, callee_file, callee_line)) = callee_info {
                    // Add edge
                    graph.edges.push(CallGraphEdge {
                        from: from_id.clone(),
                        to: Some(to_id.clone()),
                        callee_name: callee_name.clone(),
                        call_site_line: call_line,
                        call_site_column: 0,
                        call_site_file: file_path.clone(),
                        confidence: CallConfidence::Certain,
                        reason: None,
                    });

                    // Add node if not visited
                    if !visited.contains(&callee_name) {
                        visited.insert(callee_name.clone());
                        graph.nodes.push(CallGraphNode {
                            id: to_id,
                            name: callee_name.clone(),
                            file_path: callee_file,
                            line: callee_line,
                            depth: depth + 1,
                        });
                        queue.push_back((callee_name, depth + 1));
                    }
                }
            }
        }

        Ok(graph)
    }

    fn find_dead_code(&self) -> Result<DeadCodeReport> {
        let conn = self.conn.lock().unwrap();

        // Find unused functions (private functions without incoming call references)
        // Exclude: main, test_*, __init__, new, and other common entry points
        let mut stmt = conn.prepare(
            r#"
            SELECT s.id, s.name, s.kind, s.file_path, s.start_line, s.start_column,
                   s.end_line, s.end_column, s.language, s.visibility, s.signature,
                   s.doc_comment, s.parent
            FROM symbols s
            LEFT JOIN symbol_references sr ON s.name = sr.symbol_name AND sr.reference_kind = 'call'
            WHERE s.kind IN ('function', 'method')
              AND (s.visibility IS NULL OR s.visibility IN ('private', 'internal'))
              AND sr.symbol_name IS NULL
              AND s.name NOT LIKE 'test_%'
              AND s.name NOT IN ('main', '__init__', 'new', 'default', 'drop', 'clone', 'fmt', 'from', 'into')
            ORDER BY s.file_path, s.start_line
            "#,
        )?;

        let unused_functions = stmt
            .query_map([], Self::symbol_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        // Find unused types (private types without type_use references)
        let mut stmt = conn.prepare(
            r#"
            SELECT s.id, s.name, s.kind, s.file_path, s.start_line, s.start_column,
                   s.end_line, s.end_column, s.language, s.visibility, s.signature,
                   s.doc_comment, s.parent
            FROM symbols s
            LEFT JOIN symbol_references sr ON s.name = sr.symbol_name AND sr.reference_kind = 'type_use'
            WHERE s.kind IN ('struct', 'class', 'interface', 'trait', 'enum', 'type_alias')
              AND (s.visibility IS NULL OR s.visibility IN ('private', 'internal'))
              AND sr.symbol_name IS NULL
            ORDER BY s.file_path, s.start_line
            "#,
        )?;

        let unused_types = stmt
            .query_map([], Self::symbol_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(DeadCodeReport::new(unused_functions, unused_types))
    }

    fn get_function_metrics(&self, function_name: &str) -> Result<Vec<FunctionMetrics>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT name, file_path, start_line, end_line, language, signature
            FROM symbols
            WHERE name = ?1 AND kind IN ('function', 'method')
            "#,
        )?;

        let metrics = stmt
            .query_map(params![function_name], |row| {
                let name: String = row.get(0)?;
                let file_path: String = row.get(1)?;
                let start_line: u32 = row.get(2)?;
                let end_line: u32 = row.get(3)?;
                let language: String = row.get(4)?;
                let signature: Option<String> = row.get(5)?;

                // Count parameters from signature
                let param_count = signature
                    .as_ref()
                    .map(|s| Self::count_parameters(s))
                    .unwrap_or(0);

                Ok(FunctionMetrics {
                    name,
                    file_path,
                    loc: end_line.saturating_sub(start_line) + 1,
                    parameters: param_count,
                    start_line,
                    end_line,
                    language,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(metrics)
    }

    fn get_file_metrics(&self, file_path: &str) -> Result<Vec<FunctionMetrics>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT name, file_path, start_line, end_line, language, signature
            FROM symbols
            WHERE file_path = ?1 AND kind IN ('function', 'method')
            ORDER BY start_line
            "#,
        )?;

        let metrics = stmt
            .query_map(params![file_path], |row| {
                let name: String = row.get(0)?;
                let file_path: String = row.get(1)?;
                let start_line: u32 = row.get(2)?;
                let end_line: u32 = row.get(3)?;
                let language: String = row.get(4)?;
                let signature: Option<String> = row.get(5)?;

                let param_count = signature
                    .as_ref()
                    .map(|s| Self::count_parameters(s))
                    .unwrap_or(0);

                Ok(FunctionMetrics {
                    name,
                    file_path,
                    loc: end_line.saturating_sub(start_line) + 1,
                    parameters: param_count,
                    start_line,
                    end_line,
                    language,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(metrics)
    }

    fn get_all_config_digests(&self) -> Result<Vec<crate::docs::ConfigDigest>> {
        // Forward to the inherent impl
        SqliteIndex::get_all_config_digests(self)
    }
}

impl SqliteIndex {
    /// Count parameters in a function signature
    fn count_parameters(signature: &str) -> u32 {
        // Find the parameters section (between first ( and matching ))
        if let Some(start) = signature.find('(') {
            if let Some(end) = signature.rfind(')') {
                let params_str = &signature[start + 1..end];
                if params_str.trim().is_empty() {
                    return 0;
                }
                // Count commas + 1 for parameters
                // This is a simple heuristic, handles most cases
                let mut count = 1u32;
                let mut depth: i32 = 0;
                for c in params_str.chars() {
                    match c {
                        '(' | '<' | '[' | '{' => depth += 1,
                        ')' | '>' | ']' | '}' => depth = (depth - 1).max(0),
                        ',' if depth == 0 => count += 1,
                        _ => {}
                    }
                }
                return count;
            }
        }
        0
    }

    // === Documentation and Configuration Digest Methods ===

    /// Adds or updates a documentation digest
    pub fn add_doc_digest(&self, digest: &crate::docs::DocDigest) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let headings_json = serde_json::to_string(&digest.headings).unwrap_or_default();
        let command_blocks_json = serde_json::to_string(&digest.command_blocks).unwrap_or_default();
        let key_sections_json = serde_json::to_string(&digest.key_sections).unwrap_or_default();

        conn.execute(
            r#"
            INSERT OR REPLACE INTO doc_digests
            (file_path, doc_type, title, headings, command_blocks, key_sections, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                digest.file_path,
                digest.doc_type.as_str(),
                digest.title,
                headings_json,
                command_blocks_json,
                key_sections_json,
                now,
            ],
        )?;
        Ok(())
    }

    /// Gets a documentation digest by file path
    pub fn get_doc_digest(&self, file_path: &str) -> Result<Option<crate::docs::DocDigest>> {
        let conn = self.conn.lock().unwrap();

        let result = conn.query_row(
            r#"
            SELECT file_path, doc_type, title, headings, command_blocks, key_sections
            FROM doc_digests WHERE file_path = ?1
            "#,
            params![file_path],
            |row| {
                let file_path: String = row.get(0)?;
                let doc_type_str: String = row.get(1)?;
                let title: Option<String> = row.get(2)?;
                let headings_json: String = row.get(3)?;
                let command_blocks_json: String = row.get(4)?;
                let key_sections_json: String = row.get(5)?;

                let doc_type = match doc_type_str.as_str() {
                    "readme" => crate::docs::DocType::Readme,
                    "contributing" => crate::docs::DocType::Contributing,
                    "changelog" => crate::docs::DocType::Changelog,
                    "license" => crate::docs::DocType::License,
                    _ => crate::docs::DocType::Other,
                };

                let headings: Vec<crate::docs::Heading> =
                    serde_json::from_str(&headings_json).unwrap_or_default();
                let command_blocks: Vec<crate::docs::CodeBlock> =
                    serde_json::from_str(&command_blocks_json).unwrap_or_default();
                let key_sections: Vec<crate::docs::KeySection> =
                    serde_json::from_str(&key_sections_json).unwrap_or_default();

                Ok(crate::docs::DocDigest {
                    file_path,
                    doc_type,
                    title,
                    headings,
                    command_blocks,
                    key_sections,
                })
            },
        ).optional()?;

        Ok(result)
    }

    /// Gets all documentation digests
    pub fn get_all_doc_digests(&self) -> Result<Vec<crate::docs::DocDigest>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT file_path, doc_type, title, headings, command_blocks, key_sections
            FROM doc_digests ORDER BY doc_type
            "#,
        )?;

        let digests = stmt.query_map([], |row| {
            let file_path: String = row.get(0)?;
            let doc_type_str: String = row.get(1)?;
            let title: Option<String> = row.get(2)?;
            let headings_json: String = row.get(3)?;
            let command_blocks_json: String = row.get(4)?;
            let key_sections_json: String = row.get(5)?;

            let doc_type = match doc_type_str.as_str() {
                "readme" => crate::docs::DocType::Readme,
                "contributing" => crate::docs::DocType::Contributing,
                "changelog" => crate::docs::DocType::Changelog,
                "license" => crate::docs::DocType::License,
                _ => crate::docs::DocType::Other,
            };

            let headings: Vec<crate::docs::Heading> =
                serde_json::from_str(&headings_json).unwrap_or_default();
            let command_blocks: Vec<crate::docs::CodeBlock> =
                serde_json::from_str(&command_blocks_json).unwrap_or_default();
            let key_sections: Vec<crate::docs::KeySection> =
                serde_json::from_str(&key_sections_json).unwrap_or_default();

            Ok(crate::docs::DocDigest {
                file_path,
                doc_type,
                title,
                headings,
                command_blocks,
                key_sections,
            })
        })?.filter_map(|r| r.ok()).collect();

        Ok(digests)
    }

    /// Adds or updates a configuration digest
    pub fn add_config_digest(&self, digest: &crate::docs::ConfigDigest) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let scripts_json = serde_json::to_string(&digest.scripts).unwrap_or_default();
        let build_targets_json = serde_json::to_string(&digest.build_targets).unwrap_or_default();
        let test_commands_json = serde_json::to_string(&digest.test_commands).unwrap_or_default();
        let run_commands_json = serde_json::to_string(&digest.run_commands).unwrap_or_default();

        conn.execute(
            r#"
            INSERT OR REPLACE INTO config_digests
            (file_path, config_type, name, version, scripts, build_targets, test_commands, run_commands, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                digest.file_path,
                digest.config_type.as_str(),
                digest.name,
                digest.version,
                scripts_json,
                build_targets_json,
                test_commands_json,
                run_commands_json,
                now,
            ],
        )?;
        Ok(())
    }

    /// Gets a configuration digest by file path
    pub fn get_config_digest(&self, file_path: &str) -> Result<Option<crate::docs::ConfigDigest>> {
        let conn = self.conn.lock().unwrap();

        let result = conn.query_row(
            r#"
            SELECT file_path, config_type, name, version, scripts, build_targets, test_commands, run_commands
            FROM config_digests WHERE file_path = ?1
            "#,
            params![file_path],
            |row| {
                let file_path: String = row.get(0)?;
                let config_type_str: String = row.get(1)?;
                let name: Option<String> = row.get(2)?;
                let version: Option<String> = row.get(3)?;
                let scripts_json: String = row.get(4)?;
                let build_targets_json: String = row.get(5)?;
                let test_commands_json: String = row.get(6)?;
                let run_commands_json: String = row.get(7)?;

                let config_type = match config_type_str.as_str() {
                    "package_json" => crate::docs::ConfigType::PackageJson,
                    "cargo_toml" => crate::docs::ConfigType::CargoToml,
                    "makefile" => crate::docs::ConfigType::Makefile,
                    "pyproject_toml" => crate::docs::ConfigType::PyProjectToml,
                    "go_mod" => crate::docs::ConfigType::GoMod,
                    _ => crate::docs::ConfigType::Other,
                };

                let scripts: std::collections::HashMap<String, String> =
                    serde_json::from_str(&scripts_json).unwrap_or_default();
                let build_targets: Vec<String> =
                    serde_json::from_str(&build_targets_json).unwrap_or_default();
                let test_commands: Vec<String> =
                    serde_json::from_str(&test_commands_json).unwrap_or_default();
                let run_commands: Vec<String> =
                    serde_json::from_str(&run_commands_json).unwrap_or_default();

                Ok(crate::docs::ConfigDigest {
                    file_path,
                    config_type,
                    name,
                    version,
                    scripts,
                    build_targets,
                    test_commands,
                    run_commands,
                })
            },
        ).optional()?;

        Ok(result)
    }

    /// Gets all configuration digests
    pub fn get_all_config_digests(&self) -> Result<Vec<crate::docs::ConfigDigest>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT file_path, config_type, name, version, scripts, build_targets, test_commands, run_commands
            FROM config_digests ORDER BY config_type
            "#,
        )?;

        let digests = stmt.query_map([], |row| {
            let file_path: String = row.get(0)?;
            let config_type_str: String = row.get(1)?;
            let name: Option<String> = row.get(2)?;
            let version: Option<String> = row.get(3)?;
            let scripts_json: String = row.get(4)?;
            let build_targets_json: String = row.get(5)?;
            let test_commands_json: String = row.get(6)?;
            let run_commands_json: String = row.get(7)?;

            let config_type = match config_type_str.as_str() {
                "package_json" => crate::docs::ConfigType::PackageJson,
                "cargo_toml" => crate::docs::ConfigType::CargoToml,
                "makefile" => crate::docs::ConfigType::Makefile,
                "pyproject_toml" => crate::docs::ConfigType::PyProjectToml,
                "go_mod" => crate::docs::ConfigType::GoMod,
                _ => crate::docs::ConfigType::Other,
            };

            let scripts: std::collections::HashMap<String, String> =
                serde_json::from_str(&scripts_json).unwrap_or_default();
            let build_targets: Vec<String> =
                serde_json::from_str(&build_targets_json).unwrap_or_default();
            let test_commands: Vec<String> =
                serde_json::from_str(&test_commands_json).unwrap_or_default();
            let run_commands: Vec<String> =
                serde_json::from_str(&run_commands_json).unwrap_or_default();

            Ok(crate::docs::ConfigDigest {
                file_path,
                config_type,
                name,
                version,
                scripts,
                build_targets,
                test_commands,
                run_commands,
            })
        })?.filter_map(|r| r.ok()).collect();

        Ok(digests)
    }

    /// Gets aggregated commands from all config digests
    pub fn get_project_commands(&self) -> Result<ProjectCommands> {
        let digests = self.get_all_config_digests()?;

        let mut run = Vec::new();
        let mut build = Vec::new();
        let mut test = Vec::new();

        for digest in digests {
            run.extend(digest.run_commands);
            build.extend(digest.build_targets.iter().map(|t| {
                match digest.config_type {
                    crate::docs::ConfigType::PackageJson => format!("npm run {}", t),
                    crate::docs::ConfigType::CargoToml => format!("cargo build"),
                    crate::docs::ConfigType::Makefile => format!("make {}", t),
                    crate::docs::ConfigType::PyProjectToml => format!("python -m build"),
                    crate::docs::ConfigType::GoMod => format!("go build"),
                    crate::docs::ConfigType::Other => t.clone(),
                }
            }));
            test.extend(digest.test_commands);
        }

        // Deduplicate
        run.sort();
        run.dedup();
        build.sort();
        build.dedup();
        test.sort();
        test.dedup();

        Ok(ProjectCommands { run, build, test })
    }

    // === Project Compass Methods ===

    /// Saves a project profile
    pub fn save_project_profile(&self, project_path: &str, profile: &crate::compass::ProjectProfile) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let languages_json = serde_json::to_string(&profile.languages).unwrap_or_default();
        let frameworks_json = serde_json::to_string(&profile.frameworks).unwrap_or_default();
        let build_tools_json = serde_json::to_string(&profile.build_tools).unwrap_or_default();

        conn.execute(
            r#"
            INSERT OR REPLACE INTO project_profile
            (project_path, languages, frameworks, build_tools, workspace_type, profile_rev, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, COALESCE((SELECT profile_rev FROM project_profile WHERE project_path = ?1), 0) + 1, ?6)
            "#,
            params![
                project_path,
                languages_json,
                frameworks_json,
                build_tools_json,
                profile.workspace_type,
                now,
            ],
        )?;
        Ok(())
    }

    /// Gets the project profile
    pub fn get_project_profile(&self, project_path: &str) -> Result<Option<(crate::compass::ProjectProfile, u64)>> {
        let conn = self.conn.lock().unwrap();

        let result = conn.query_row(
            r#"
            SELECT languages, frameworks, build_tools, workspace_type, profile_rev
            FROM project_profile WHERE project_path = ?1
            "#,
            params![project_path],
            |row| {
                let languages_json: String = row.get(0)?;
                let frameworks_json: String = row.get(1)?;
                let build_tools_json: String = row.get(2)?;
                let workspace_type: Option<String> = row.get(3)?;
                let profile_rev: i64 = row.get(4)?;

                let languages: Vec<crate::compass::LanguageStats> =
                    serde_json::from_str(&languages_json).unwrap_or_default();
                let frameworks: Vec<crate::compass::FrameworkInfo> =
                    serde_json::from_str(&frameworks_json).unwrap_or_default();
                let build_tools: Vec<String> =
                    serde_json::from_str(&build_tools_json).unwrap_or_default();

                let total_files = languages.iter().map(|l| l.file_count).sum();
                let total_symbols = languages.iter().map(|l| l.symbol_count).sum();

                Ok((
                    crate::compass::ProjectProfile {
                        languages,
                        frameworks,
                        build_tools,
                        workspace_type,
                        total_files,
                        total_symbols,
                    },
                    profile_rev as u64,
                ))
            },
        ).optional()?;

        Ok(result)
    }

    /// Saves project nodes
    pub fn save_project_nodes(&self, nodes: &[crate::compass::ProjectNode]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // Clear existing nodes
        tx.execute("DELETE FROM project_nodes", [])?;

        for node in nodes {
            tx.execute(
                r#"
                INSERT INTO project_nodes
                (id, parent_id, node_type, name, path, symbol_count, public_symbol_count, file_count, centrality_score)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    node.id,
                    node.parent_id,
                    node.node_type.as_str(),
                    node.name,
                    node.path,
                    node.symbol_count as i64,
                    node.public_symbol_count as i64,
                    node.file_count as i64,
                    node.centrality_score as f64,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Gets all project nodes
    pub fn get_project_nodes(&self) -> Result<Vec<crate::compass::ProjectNode>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, parent_id, node_type, name, path, symbol_count, public_symbol_count, file_count, centrality_score
            FROM project_nodes ORDER BY symbol_count DESC
            "#,
        )?;

        let nodes = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let parent_id: Option<String> = row.get(1)?;
            let node_type_str: String = row.get(2)?;
            let name: String = row.get(3)?;
            let path: String = row.get(4)?;
            let symbol_count: i64 = row.get(5)?;
            let public_symbol_count: i64 = row.get(6)?;
            let file_count: i64 = row.get(7)?;
            let centrality_score: f64 = row.get(8)?;

            let node_type = match node_type_str.as_str() {
                "module" => crate::compass::NodeType::Module,
                "directory" => crate::compass::NodeType::Directory,
                "package" => crate::compass::NodeType::Package,
                "layer" => crate::compass::NodeType::Layer,
                _ => crate::compass::NodeType::Directory,
            };

            Ok(crate::compass::ProjectNode {
                id,
                parent_id,
                node_type,
                name,
                path,
                symbol_count: symbol_count as usize,
                public_symbol_count: public_symbol_count as usize,
                file_count: file_count as usize,
                centrality_score: centrality_score as f32,
                children: Vec::new(), // Will be populated separately
            })
        })?.filter_map(|r| r.ok()).collect();

        Ok(nodes)
    }

    /// Gets a single project node by ID
    pub fn get_project_node(&self, node_id: &str) -> Result<Option<crate::compass::ProjectNode>> {
        let conn = self.conn.lock().unwrap();

        let result = conn.query_row(
            r#"
            SELECT id, parent_id, node_type, name, path, symbol_count, public_symbol_count, file_count, centrality_score
            FROM project_nodes WHERE id = ?1
            "#,
            params![node_id],
            |row| {
                let id: String = row.get(0)?;
                let parent_id: Option<String> = row.get(1)?;
                let node_type_str: String = row.get(2)?;
                let name: String = row.get(3)?;
                let path: String = row.get(4)?;
                let symbol_count: i64 = row.get(5)?;
                let public_symbol_count: i64 = row.get(6)?;
                let file_count: i64 = row.get(7)?;
                let centrality_score: f64 = row.get(8)?;

                let node_type = match node_type_str.as_str() {
                    "module" => crate::compass::NodeType::Module,
                    "directory" => crate::compass::NodeType::Directory,
                    "package" => crate::compass::NodeType::Package,
                    "layer" => crate::compass::NodeType::Layer,
                    _ => crate::compass::NodeType::Directory,
                };

                Ok(crate::compass::ProjectNode {
                    id,
                    parent_id,
                    node_type,
                    name,
                    path,
                    symbol_count: symbol_count as usize,
                    public_symbol_count: public_symbol_count as usize,
                    file_count: file_count as usize,
                    centrality_score: centrality_score as f32,
                    children: Vec::new(),
                })
            },
        ).optional()?;

        Ok(result)
    }

    /// Gets children of a project node
    pub fn get_node_children(&self, parent_id: &str) -> Result<Vec<crate::compass::ProjectNode>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, parent_id, node_type, name, path, symbol_count, public_symbol_count, file_count, centrality_score
            FROM project_nodes WHERE parent_id = ?1 ORDER BY symbol_count DESC
            "#,
        )?;

        let nodes = stmt.query_map(params![parent_id], |row| {
            let id: String = row.get(0)?;
            let parent_id: Option<String> = row.get(1)?;
            let node_type_str: String = row.get(2)?;
            let name: String = row.get(3)?;
            let path: String = row.get(4)?;
            let symbol_count: i64 = row.get(5)?;
            let public_symbol_count: i64 = row.get(6)?;
            let file_count: i64 = row.get(7)?;
            let centrality_score: f64 = row.get(8)?;

            let node_type = match node_type_str.as_str() {
                "module" => crate::compass::NodeType::Module,
                "directory" => crate::compass::NodeType::Directory,
                "package" => crate::compass::NodeType::Package,
                "layer" => crate::compass::NodeType::Layer,
                _ => crate::compass::NodeType::Directory,
            };

            Ok(crate::compass::ProjectNode {
                id,
                parent_id,
                node_type,
                name,
                path,
                symbol_count: symbol_count as usize,
                public_symbol_count: public_symbol_count as usize,
                file_count: file_count as usize,
                centrality_score: centrality_score as f32,
                children: Vec::new(),
            })
        })?.filter_map(|r| r.ok()).collect();

        Ok(nodes)
    }

    /// Saves entry points
    pub fn save_entry_points(&self, entries: &[crate::compass::EntryPoint]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // Clear existing entry points
        tx.execute("DELETE FROM entry_points", [])?;

        for entry in entries {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO entry_points
                (symbol_id, entry_type, file_path, line, name, evidence)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    entry.symbol_id,
                    entry.entry_type.as_str(),
                    entry.file_path,
                    entry.line as i64,
                    entry.name,
                    entry.evidence,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Gets all entry points
    pub fn get_entry_points(&self) -> Result<Vec<crate::compass::EntryPoint>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT symbol_id, entry_type, file_path, line, name, evidence
            FROM entry_points ORDER BY entry_type, file_path
            "#,
        )?;

        let entries = stmt.query_map([], |row| {
            let symbol_id: Option<String> = row.get(0)?;
            let entry_type_str: String = row.get(1)?;
            let file_path: String = row.get(2)?;
            let line: i64 = row.get(3)?;
            let name: String = row.get(4)?;
            let evidence: Option<String> = row.get(5)?;

            let entry_type = match entry_type_str.as_str() {
                "main" => crate::compass::EntryType::Main,
                "tokio_main" => crate::compass::EntryType::TokioMain,
                "actix_main" => crate::compass::EntryType::ActixMain,
                "server" => crate::compass::EntryType::Server,
                "cli" => crate::compass::EntryType::Cli,
                "rest_endpoint" => crate::compass::EntryType::RestEndpoint,
                "graphql_resolver" => crate::compass::EntryType::GraphqlResolver,
                "grpc_service" => crate::compass::EntryType::GrpcService,
                "test" => crate::compass::EntryType::Test,
                "benchmark" => crate::compass::EntryType::Benchmark,
                _ => crate::compass::EntryType::Main,
            };

            Ok(crate::compass::EntryPoint {
                symbol_id,
                entry_type,
                file_path,
                line: line as u32,
                name,
                evidence: evidence.unwrap_or_default(),
            })
        })?.filter_map(|r| r.ok()).collect();

        Ok(entries)
    }
}

/// Aggregated project commands
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectCommands {
    pub run: Vec<String>,
    pub build: Vec<String>,
    pub test: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Location;

    fn create_test_symbol(name: &str, kind: SymbolKind, file: &str, language: &str) -> Symbol {
        Symbol::new(name, kind, Location::new(file, 1, 0, 5, 1), language)
    }

    #[test]
    fn test_add_and_get_symbol() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbol = Symbol::new(
            "test_function",
            SymbolKind::Function,
            Location::new("test.rs", 1, 0, 5, 1),
            "rust",
        )
        .with_visibility(Visibility::Public)
        .with_signature("fn test_function() -> i32");

        let id = symbol.id.clone();
        index.add_symbol(symbol).unwrap();

        let retrieved = index.get_symbol(&id).unwrap().unwrap();
        assert_eq!(retrieved.name, "test_function");
        assert_eq!(retrieved.kind, SymbolKind::Function);
    }

    #[test]
    fn test_get_symbol_not_found() {
        let index = SqliteIndex::in_memory().unwrap();
        let result = index.get_symbol("non-existent-id").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_search() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbol1 = Symbol::new(
            "calculate_sum",
            SymbolKind::Function,
            Location::new("math.rs", 1, 0, 5, 1),
            "rust",
        );

        let symbol2 = Symbol::new(
            "calculate_product",
            SymbolKind::Function,
            Location::new("math.rs", 10, 0, 15, 1),
            "rust",
        );

        index.add_symbols(vec![symbol1, symbol2]).unwrap();

        let results = index.search("calculate", &SearchOptions::default()).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_add_symbols_batch() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            create_test_symbol("func1", SymbolKind::Function, "test.rs", "rust"),
            create_test_symbol("func2", SymbolKind::Function, "test.rs", "rust"),
            create_test_symbol("func3", SymbolKind::Function, "test.rs", "rust"),
        ];

        index.add_symbols(symbols).unwrap();

        let stats = index.get_stats().unwrap();
        assert_eq!(stats.total_symbols, 3);
    }

    #[test]
    fn test_add_symbols_empty() {
        let index = SqliteIndex::in_memory().unwrap();
        index.add_symbols(vec![]).unwrap();
        let stats = index.get_stats().unwrap();
        assert_eq!(stats.total_symbols, 0);
    }

    #[test]
    fn test_remove_file() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            create_test_symbol("func1", SymbolKind::Function, "file1.rs", "rust"),
            create_test_symbol("func2", SymbolKind::Function, "file1.rs", "rust"),
            create_test_symbol("func3", SymbolKind::Function, "file2.rs", "rust"),
        ];
        index.add_symbols(symbols).unwrap();

        index.remove_file("file1.rs").unwrap();

        let stats = index.get_stats().unwrap();
        assert_eq!(stats.total_symbols, 1);
    }

    #[test]
    fn test_remove_file_not_exists() {
        let index = SqliteIndex::in_memory().unwrap();
        index.remove_file("non-existent.rs").unwrap();
    }

    #[test]
    fn test_find_definition() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            Symbol::new(
                "MyStruct",
                SymbolKind::Struct,
                Location::new("lib.rs", 1, 0, 10, 1),
                "rust",
            ),
            Symbol::new(
                "MyStruct",
                SymbolKind::Import,
                Location::new("main.rs", 1, 0, 1, 20),
                "rust",
            ),
        ];
        index.add_symbols(symbols).unwrap();

        let defs = index.find_definition("MyStruct").unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].kind, SymbolKind::Struct);
    }

    #[test]
    fn test_find_definition_excludes_variables() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            Symbol::new(
                "config",
                SymbolKind::Constant,
                Location::new("lib.rs", 1, 0, 1, 20),
                "rust",
            ),
            Symbol::new(
                "config",
                SymbolKind::Variable,
                Location::new("main.rs", 5, 0, 5, 15),
                "rust",
            ),
        ];
        index.add_symbols(symbols).unwrap();

        let defs = index.find_definition("config").unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].kind, SymbolKind::Constant);
    }

    #[test]
    fn test_find_definition_not_found() {
        let index = SqliteIndex::in_memory().unwrap();
        let defs = index.find_definition("NonExistent").unwrap();
        assert!(defs.is_empty());
    }

    #[test]
    fn test_list_functions() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            create_test_symbol("func1", SymbolKind::Function, "test.rs", "rust"),
            create_test_symbol("method1", SymbolKind::Method, "test.rs", "rust"),
            create_test_symbol("MyStruct", SymbolKind::Struct, "test.rs", "rust"),
        ];
        index.add_symbols(symbols).unwrap();

        let funcs = index.list_functions(&SearchOptions::default()).unwrap();
        assert_eq!(funcs.len(), 2);
    }

    #[test]
    fn test_list_functions_with_limit() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            create_test_symbol("func1", SymbolKind::Function, "test.rs", "rust"),
            create_test_symbol("func2", SymbolKind::Function, "test.rs", "rust"),
            create_test_symbol("func3", SymbolKind::Function, "test.rs", "rust"),
        ];
        index.add_symbols(symbols).unwrap();

        let options = SearchOptions {
            limit: Some(2),
            ..Default::default()
        };
        let funcs = index.list_functions(&options).unwrap();
        assert_eq!(funcs.len(), 2);
    }

    #[test]
    fn test_list_functions_with_language_filter() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            create_test_symbol("rust_func", SymbolKind::Function, "test.rs", "rust"),
            create_test_symbol("java_func", SymbolKind::Function, "Test.java", "java"),
        ];
        index.add_symbols(symbols).unwrap();

        let options = SearchOptions {
            language_filter: Some(vec!["rust".to_string()]),
            ..Default::default()
        };
        let funcs = index.list_functions(&options).unwrap();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "rust_func");
    }

    #[test]
    fn test_list_functions_with_file_filter() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            create_test_symbol("func1", SymbolKind::Function, "src/lib.rs", "rust"),
            create_test_symbol("func2", SymbolKind::Function, "src/main.rs", "rust"),
            create_test_symbol("func3", SymbolKind::Function, "tests/test.rs", "rust"),
        ];
        index.add_symbols(symbols).unwrap();

        let options = SearchOptions {
            file_filter: Some("src".to_string()),
            ..Default::default()
        };
        let funcs = index.list_functions(&options).unwrap();
        assert_eq!(funcs.len(), 2);
    }

    #[test]
    fn test_list_types() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            create_test_symbol("MyStruct", SymbolKind::Struct, "test.rs", "rust"),
            create_test_symbol("MyClass", SymbolKind::Class, "Test.java", "java"),
            create_test_symbol("MyInterface", SymbolKind::Interface, "test.ts", "typescript"),
            create_test_symbol("MyTrait", SymbolKind::Trait, "test.rs", "rust"),
            create_test_symbol("MyEnum", SymbolKind::Enum, "test.rs", "rust"),
            create_test_symbol("MyAlias", SymbolKind::TypeAlias, "test.rs", "rust"),
            create_test_symbol("my_func", SymbolKind::Function, "test.rs", "rust"),
        ];
        index.add_symbols(symbols).unwrap();

        let types = index.list_types(&SearchOptions::default()).unwrap();
        assert_eq!(types.len(), 6);
    }

    #[test]
    fn test_list_types_with_language_filter() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            create_test_symbol("RustStruct", SymbolKind::Struct, "test.rs", "rust"),
            create_test_symbol("JavaClass", SymbolKind::Class, "Test.java", "java"),
        ];
        index.add_symbols(symbols).unwrap();

        let options = SearchOptions {
            language_filter: Some(vec!["java".to_string()]),
            ..Default::default()
        };
        let types = index.list_types(&options).unwrap();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].name, "JavaClass");
    }

    #[test]
    fn test_get_file_symbols() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            Symbol::new(
                "func1",
                SymbolKind::Function,
                Location::new("test.rs", 1, 0, 5, 1),
                "rust",
            ),
            Symbol::new(
                "func2",
                SymbolKind::Function,
                Location::new("test.rs", 10, 0, 15, 1),
                "rust",
            ),
            Symbol::new(
                "other",
                SymbolKind::Function,
                Location::new("other.rs", 1, 0, 5, 1),
                "rust",
            ),
        ];
        index.add_symbols(symbols).unwrap();

        let file_symbols = index.get_file_symbols("test.rs").unwrap();
        assert_eq!(file_symbols.len(), 2);
        assert!(file_symbols[0].location.start_line < file_symbols[1].location.start_line);
    }

    #[test]
    fn test_get_file_symbols_not_found() {
        let index = SqliteIndex::in_memory().unwrap();
        let symbols = index.get_file_symbols("non-existent.rs").unwrap();
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_get_stats() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            create_test_symbol("func1", SymbolKind::Function, "test.rs", "rust"),
            create_test_symbol("func2", SymbolKind::Function, "test.rs", "rust"),
            create_test_symbol("MyStruct", SymbolKind::Struct, "test.rs", "rust"),
            create_test_symbol("JavaClass", SymbolKind::Class, "Test.java", "java"),
        ];
        index.add_symbols(symbols).unwrap();

        let stats = index.get_stats().unwrap();
        assert_eq!(stats.total_symbols, 4);
        assert_eq!(stats.total_files, 2);
        assert!(stats.symbols_by_kind.iter().any(|(k, c)| k == "function" && *c == 2));
        assert!(stats.symbols_by_language.iter().any(|(l, c)| l == "rust" && *c == 3));
        assert!(stats.files_by_language.iter().any(|(l, c)| l == "rust" && *c == 1));
    }

    #[test]
    fn test_get_stats_empty() {
        let index = SqliteIndex::in_memory().unwrap();
        let stats = index.get_stats().unwrap();
        assert_eq!(stats.total_symbols, 0);
        assert_eq!(stats.total_files, 0);
    }

    #[test]
    fn test_clear() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            create_test_symbol("func1", SymbolKind::Function, "test.rs", "rust"),
            create_test_symbol("func2", SymbolKind::Function, "test.rs", "rust"),
        ];
        index.add_symbols(symbols).unwrap();

        index.clear().unwrap();

        let stats = index.get_stats().unwrap();
        assert_eq!(stats.total_symbols, 0);
    }

    #[test]
    fn test_search_with_kind_filter() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            Symbol::new(
                "my_function",
                SymbolKind::Function,
                Location::new("test.rs", 1, 0, 5, 1),
                "rust",
            ),
            Symbol::new(
                "my_struct",
                SymbolKind::Struct,
                Location::new("test.rs", 10, 0, 20, 1),
                "rust",
            ),
        ];
        index.add_symbols(symbols).unwrap();

        let options = SearchOptions {
            kind_filter: Some(vec![SymbolKind::Function]),
            ..Default::default()
        };
        let results = index.search("my", &options).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.kind, SymbolKind::Function);
    }

    #[test]
    fn test_search_with_language_filter() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            Symbol::new(
                "rust_func",
                SymbolKind::Function,
                Location::new("test.rs", 1, 0, 5, 1),
                "rust",
            ),
            Symbol::new(
                "java_func",
                SymbolKind::Function,
                Location::new("Test.java", 1, 0, 5, 1),
                "java",
            ),
        ];
        index.add_symbols(symbols).unwrap();

        let options = SearchOptions {
            language_filter: Some(vec!["rust".to_string()]),
            ..Default::default()
        };
        let results = index.search("func", &options).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.language, "rust");
    }

    #[test]
    fn test_search_with_file_filter() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            Symbol::new(
                "func1",
                SymbolKind::Function,
                Location::new("src/lib.rs", 1, 0, 5, 1),
                "rust",
            ),
            Symbol::new(
                "func2",
                SymbolKind::Function,
                Location::new("tests/test.rs", 1, 0, 5, 1),
                "rust",
            ),
        ];
        index.add_symbols(symbols).unwrap();

        let options = SearchOptions {
            file_filter: Some("src".to_string()),
            ..Default::default()
        };
        let results = index.search("func", &options).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].symbol.location.file_path.contains("src"));
    }

    #[test]
    fn test_search_with_limit() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbols = vec![
            Symbol::new(
                "func_a",
                SymbolKind::Function,
                Location::new("test.rs", 1, 0, 5, 1),
                "rust",
            ),
            Symbol::new(
                "func_b",
                SymbolKind::Function,
                Location::new("test.rs", 10, 0, 15, 1),
                "rust",
            ),
            Symbol::new(
                "func_c",
                SymbolKind::Function,
                Location::new("test.rs", 20, 0, 25, 1),
                "rust",
            ),
        ];
        index.add_symbols(symbols).unwrap();

        let options = SearchOptions {
            limit: Some(2),
            ..Default::default()
        };
        let results = index.search("func", &options).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_no_results() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbol = create_test_symbol("my_function", SymbolKind::Function, "test.rs", "rust");
        index.add_symbol(symbol).unwrap();

        let results = index.search("nonexistent", &SearchOptions::default()).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_score_positive() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbol = Symbol::new(
            "calculate",
            SymbolKind::Function,
            Location::new("test.rs", 1, 0, 5, 1),
            "rust",
        );
        index.add_symbol(symbol).unwrap();

        let results = index.search("calculate", &SearchOptions::default()).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].score > 0.0);
    }

    #[test]
    fn test_symbol_with_all_fields() {
        let index = SqliteIndex::in_memory().unwrap();

        let symbol = Symbol::new(
            "complex_method",
            SymbolKind::Method,
            Location::new("test.rs", 10, 4, 25, 5),
            "rust",
        )
        .with_visibility(Visibility::Public)
        .with_signature("fn complex_method(&self, x: i32) -> Result<String, Error>")
        .with_doc_comment("/// A complex method with many features")
        .with_parent("MyStruct");

        let id = symbol.id.clone();
        index.add_symbol(symbol).unwrap();

        let retrieved = index.get_symbol(&id).unwrap().unwrap();
        assert_eq!(retrieved.name, "complex_method");
        assert_eq!(retrieved.kind, SymbolKind::Method);
        assert_eq!(retrieved.visibility, Some(Visibility::Public));
        assert_eq!(
            retrieved.signature,
            Some("fn complex_method(&self, x: i32) -> Result<String, Error>".to_string())
        );
        assert_eq!(
            retrieved.doc_comment,
            Some("/// A complex method with many features".to_string())
        );
        assert_eq!(retrieved.parent, Some("MyStruct".to_string()));
        assert_eq!(retrieved.location.start_line, 10);
        assert_eq!(retrieved.location.end_line, 25);
    }

    #[test]
    fn test_symbol_replace_on_duplicate_id() {
        let index = SqliteIndex::in_memory().unwrap();

        let mut symbol1 = Symbol::new(
            "original",
            SymbolKind::Function,
            Location::new("test.rs", 1, 0, 5, 1),
            "rust",
        );
        symbol1.id = "fixed-id".to_string();
        index.add_symbol(symbol1).unwrap();

        let mut symbol2 = Symbol::new(
            "replaced",
            SymbolKind::Function,
            Location::new("test.rs", 1, 0, 5, 1),
            "rust",
        );
        symbol2.id = "fixed-id".to_string();
        index.add_symbol(symbol2).unwrap();

        let retrieved = index.get_symbol("fixed-id").unwrap().unwrap();
        assert_eq!(retrieved.name, "replaced");
    }

    #[test]
    fn test_get_symbol_metrics_batch() {
        let index = SqliteIndex::in_memory().unwrap();

        // Create symbols
        let mut symbol1 = create_test_symbol("func1", SymbolKind::Function, "test.rs", "rust");
        let mut symbol2 = create_test_symbol("func2", SymbolKind::Function, "test.rs", "rust");
        let mut symbol3 = create_test_symbol("func3", SymbolKind::Function, "test.rs", "rust");

        symbol1.id = "id1".to_string();
        symbol2.id = "id2".to_string();
        symbol3.id = "id3".to_string();

        index.add_symbols(vec![symbol1, symbol2, symbol3]).unwrap();

        // Add metrics for two symbols
        index
            .update_symbol_metrics(&SymbolMetrics {
                symbol_id: "id1".to_string(),
                pagerank: 0.5,
                incoming_refs: 10,
                outgoing_refs: 5,
                git_recency: 0.8,
            })
            .unwrap();

        index
            .update_symbol_metrics(&SymbolMetrics {
                symbol_id: "id2".to_string(),
                pagerank: 0.3,
                incoming_refs: 3,
                outgoing_refs: 7,
                git_recency: 0.2,
            })
            .unwrap();

        // Batch query
        let ids = vec!["id1", "id2", "id3", "id_nonexistent"];
        let metrics_map = index.get_symbol_metrics_batch(&ids).unwrap();

        // Should have 2 entries (id1 and id2 have metrics, id3 doesn't)
        assert_eq!(metrics_map.len(), 2);
        assert!(metrics_map.contains_key("id1"));
        assert!(metrics_map.contains_key("id2"));
        assert!(!metrics_map.contains_key("id3"));
        assert!(!metrics_map.contains_key("id_nonexistent"));

        let m1 = metrics_map.get("id1").unwrap();
        assert_eq!(m1.pagerank, 0.5);
        assert_eq!(m1.incoming_refs, 10);

        let m2 = metrics_map.get("id2").unwrap();
        assert_eq!(m2.pagerank, 0.3);
        assert_eq!(m2.git_recency, 0.2);
    }

    #[test]
    fn test_get_symbol_metrics_batch_empty() {
        let index = SqliteIndex::in_memory().unwrap();

        // Empty input should return empty map
        let metrics_map = index.get_symbol_metrics_batch(&[]).unwrap();
        assert!(metrics_map.is_empty());
    }

    // === Database Revision Tests ===

    #[test]
    fn test_get_db_revision_initial() {
        let index = SqliteIndex::in_memory().unwrap();
        let rev = index.get_db_revision().unwrap();
        assert_eq!(rev, 0);
    }

    #[test]
    fn test_increment_db_revision() {
        let index = SqliteIndex::in_memory().unwrap();

        let rev1 = index.increment_db_revision().unwrap();
        assert_eq!(rev1, 1);

        let rev2 = index.increment_db_revision().unwrap();
        assert_eq!(rev2, 2);

        let rev3 = index.get_db_revision().unwrap();
        assert_eq!(rev3, 2);
    }

    #[test]
    fn test_db_revision_monotonic() {
        let index = SqliteIndex::in_memory().unwrap();

        let mut prev = index.get_db_revision().unwrap();
        for _ in 0..10 {
            let current = index.increment_db_revision().unwrap();
            assert!(current > prev);
            prev = current;
        }
    }

    #[test]
    fn test_db_revision_persists_between_reads() {
        let index = SqliteIndex::in_memory().unwrap();

        index.increment_db_revision().unwrap();
        index.increment_db_revision().unwrap();
        index.increment_db_revision().unwrap();

        // Multiple reads should return the same value
        let rev1 = index.get_db_revision().unwrap();
        let rev2 = index.get_db_revision().unwrap();
        let rev3 = index.get_db_revision().unwrap();

        assert_eq!(rev1, rev2);
        assert_eq!(rev2, rev3);
        assert_eq!(rev1, 3);
    }

    #[test]
    fn test_db_revision_large_numbers() {
        let index = SqliteIndex::in_memory().unwrap();

        // Increment many times
        for _ in 0..100 {
            index.increment_db_revision().unwrap();
        }

        let rev = index.get_db_revision().unwrap();
        assert_eq!(rev, 100);
    }

    // === Content Hash Tests ===

    #[test]
    fn test_compute_content_hash_deterministic() {
        let content = "fn main() {}";
        let hash1 = SqliteIndex::compute_content_hash(content);
        let hash2 = SqliteIndex::compute_content_hash(content);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_content_hash_different_content() {
        let hash1 = SqliteIndex::compute_content_hash("fn main() {}");
        let hash2 = SqliteIndex::compute_content_hash("fn main() { println!(); }");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_content_hash_empty_string() {
        let hash = SqliteIndex::compute_content_hash("");
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 16); // 16 hex chars for u64
    }

    #[test]
    fn test_compute_content_hash_unicode() {
        let hash1 = SqliteIndex::compute_content_hash("// Привет мир");
        let hash2 = SqliteIndex::compute_content_hash("// Hello world");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_content_hash_whitespace_sensitive() {
        let hash1 = SqliteIndex::compute_content_hash("fn main() {}");
        let hash2 = SqliteIndex::compute_content_hash("fn main(){}");
        let hash3 = SqliteIndex::compute_content_hash("fn main() {}\n");

        assert_ne!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_get_file_content_hash_not_found() {
        let index = SqliteIndex::in_memory().unwrap();
        let hash = index.get_file_content_hash("nonexistent.rs").unwrap();
        assert!(hash.is_none());
    }

    // Helper to add a file entry to the files table
    fn add_test_file(index: &SqliteIndex, path: &str, language: &str) {
        let conn = index.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO files (path, language, last_modified, symbol_count) VALUES (?1, ?2, ?3, ?4)",
            params![path, language, 0i64, 0i64],
        ).unwrap();
    }

    #[test]
    fn test_set_and_get_file_content_hash() {
        let index = SqliteIndex::in_memory().unwrap();

        // First add a file entry
        add_test_file(&index, "test.rs", "rust");

        // Set content hash
        let hash = "abc123def456";
        index.set_file_content_hash("test.rs", hash).unwrap();

        // Retrieve it
        let retrieved = index.get_file_content_hash("test.rs").unwrap();
        assert_eq!(retrieved, Some(hash.to_string()));
    }

    #[test]
    fn test_file_needs_reindex_new_file() {
        let index = SqliteIndex::in_memory().unwrap();

        // File not in index should need reindexing
        let needs = index.file_needs_reindex("new_file.rs", "somehash").unwrap();
        assert!(needs);
    }

    #[test]
    fn test_file_needs_reindex_same_hash() {
        let index = SqliteIndex::in_memory().unwrap();

        // Add file entry and hash
        add_test_file(&index, "test.rs", "rust");
        index.set_file_content_hash("test.rs", "abc123").unwrap();

        // Same hash - no reindex needed
        let needs = index.file_needs_reindex("test.rs", "abc123").unwrap();
        assert!(!needs);
    }

    #[test]
    fn test_file_needs_reindex_different_hash() {
        let index = SqliteIndex::in_memory().unwrap();

        // Add file entry and hash
        add_test_file(&index, "test.rs", "rust");
        index.set_file_content_hash("test.rs", "abc123").unwrap();

        // Different hash - needs reindex
        let needs = index.file_needs_reindex("test.rs", "xyz789").unwrap();
        assert!(needs);
    }

    #[test]
    fn test_file_needs_reindex_null_hash() {
        let index = SqliteIndex::in_memory().unwrap();

        // Add file entry without setting hash
        add_test_file(&index, "test.rs", "rust");

        // Null hash in DB means needs reindex
        let needs = index.file_needs_reindex("test.rs", "anyhash").unwrap();
        assert!(needs);
    }

    #[test]
    fn test_content_hash_update() {
        let index = SqliteIndex::in_memory().unwrap();

        // Add file entry
        add_test_file(&index, "test.rs", "rust");

        // Set initial hash
        index.set_file_content_hash("test.rs", "hash_v1").unwrap();
        assert_eq!(index.get_file_content_hash("test.rs").unwrap(), Some("hash_v1".to_string()));

        // Update hash
        index.set_file_content_hash("test.rs", "hash_v2").unwrap();
        assert_eq!(index.get_file_content_hash("test.rs").unwrap(), Some("hash_v2".to_string()));
    }

    #[test]
    fn test_content_hash_integration_workflow() {
        let index = SqliteIndex::in_memory().unwrap();

        let content_v1 = "fn hello() {}";
        let content_v2 = "fn hello() { println!(\"hello\"); }";

        let hash_v1 = SqliteIndex::compute_content_hash(content_v1);
        let hash_v2 = SqliteIndex::compute_content_hash(content_v2);

        // Initial indexing - add file entry and set hash
        add_test_file(&index, "lib.rs", "rust");
        index.set_file_content_hash("lib.rs", &hash_v1).unwrap();

        // Check if reindex needed with same content
        assert!(!index.file_needs_reindex("lib.rs", &hash_v1).unwrap());

        // Check if reindex needed with changed content
        assert!(index.file_needs_reindex("lib.rs", &hash_v2).unwrap());

        // After reindexing, update hash
        index.set_file_content_hash("lib.rs", &hash_v2).unwrap();
        assert!(!index.file_needs_reindex("lib.rs", &hash_v2).unwrap());
    }
}
