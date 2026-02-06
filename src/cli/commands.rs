use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use clap::{Parser, Subcommand, ValueEnum};
use rayon::prelude::*;

use crate::error::Result;
use crate::index::sqlite::{IndexedFileRecord, SqliteIndex};
use crate::index::{CodeIndex, CompactSymbol, FileMeta, MetaSource, OutputFormat, SearchOptions};
use crate::indexer::watcher::FileEvent;
use crate::indexer::{
    compute_exported_hash, extract_file_meta, extract_file_tags, parse_sidecar, resolve_tags,
    FileWalker, FileWatcher, ParseCache, Parser as CodeParser, SymbolExtractor, SIDECAR_FILENAME,
};
use crate::languages::LanguageRegistry;
use code_indexer::dependencies::{DependencyRegistry, ProjectInfo};
use code_indexer::git::GitAnalyzer;

#[derive(Parser)]
#[command(name = "code-indexer")]
#[command(about = "CLI tool for code indexing and search using tree-sitter")]
#[command(version)]
#[command(after_long_help = r#"
EXAMPLES:
    # Index current directory
    code-indexer index

    # Index specific path with watch mode
    code-indexer index ./src --watch

    # Search for symbols
    code-indexer query search "MyFunction"

    # Find definition
    code-indexer query definition "SymbolName"

    # List all functions
    code-indexer query functions --limit 50

    # List types filtered by language
    code-indexer query types --language rust

    # Show index statistics
    code-indexer stats

    # Start MCP server
    code-indexer serve
"#)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Path to the index database
    #[arg(long, default_value = ".code-index.db")]
    pub db: PathBuf,
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum ServeTransport {
    Stdio,
    Unix,
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum IndexDurability {
    Safe,
    Fast,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Index a directory
    Index {
        /// Path to the directory to index
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Watch for file changes and update index
        #[arg(long)]
        watch: bool,

        /// Also index dependencies (deep indexing)
        #[arg(long)]
        deep_deps: bool,

        /// Durability profile for bulk index write path
        #[arg(long, value_enum, default_value = "fast")]
        durability: IndexDurability,
    },

    /// Start MCP server
    Serve {
        /// Transport for MCP server
        #[arg(long, value_enum, default_value = "stdio")]
        transport: ServeTransport,

        /// Unix socket path (required when --transport unix)
        #[arg(long)]
        socket: Option<PathBuf>,
    },

    /// Search and list symbols (replaces query search/functions/types)
    Symbols {
        /// Search query (optional - if omitted, lists all symbols)
        query: Option<String>,

        /// Filter by kind: function, type, all (default: all)
        #[arg(long, short, default_value = "all")]
        kind: String,

        /// Maximum number of results
        #[arg(long, default_value = "100")]
        limit: usize,

        /// Filter by language
        #[arg(long)]
        language: Option<String>,

        /// Filter by file path
        #[arg(long)]
        file: Option<String>,

        /// Filter by name pattern (glob: * and ? supported)
        #[arg(long)]
        pattern: Option<String>,

        /// Output format: full, compact, minimal
        #[arg(long, default_value = "full")]
        format: String,

        /// Enable fuzzy search for typo tolerance
        #[arg(long)]
        fuzzy: bool,

        /// Fuzzy search threshold (0.0-1.0)
        #[arg(long, default_value = "0.7")]
        fuzzy_threshold: f64,

        /// Use already running MCP daemon over unix socket
        #[arg(long)]
        remote: Option<PathBuf>,
    },

    /// Find symbol definitions
    Definition {
        /// Symbol name
        name: String,

        /// Also search in dependencies
        #[arg(long)]
        include_deps: bool,

        /// Filter by specific dependency
        #[arg(long)]
        dep: Option<String>,

        /// Use already running MCP daemon over unix socket
        #[arg(long)]
        remote: Option<PathBuf>,
    },

    /// Find symbol references (replaces find_references + find_callers)
    References {
        /// Symbol name
        name: String,

        /// Include callers (who calls this function)
        #[arg(long)]
        callers: bool,

        /// Depth for caller search (default: 1)
        #[arg(long, default_value = "1")]
        depth: u32,

        /// Filter by file path
        #[arg(long)]
        file: Option<String>,

        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: usize,

        /// Use already running MCP daemon over unix socket
        #[arg(long)]
        remote: Option<PathBuf>,
    },

    /// Analyze call graph (replaces get_call_graph + find_callees)
    CallGraph {
        /// Entry point function name
        function: String,

        /// Direction: out (callees), in (callers), both
        #[arg(long, default_value = "out")]
        direction: String,

        /// Maximum depth (default: 3)
        #[arg(long, default_value = "3")]
        depth: u32,

        /// Include possible (uncertain) calls
        #[arg(long)]
        include_possible: bool,

        /// Use already running MCP daemon over unix socket
        #[arg(long)]
        remote: Option<PathBuf>,
    },

    /// Get file outline/structure
    Outline {
        /// File path
        file: PathBuf,

        /// Start line (for range selection)
        #[arg(long)]
        start_line: Option<u32>,

        /// End line (for range selection)
        #[arg(long)]
        end_line: Option<u32>,

        /// Include scopes
        #[arg(long)]
        scopes: bool,

        /// Use already running MCP daemon over unix socket
        #[arg(long)]
        remote: Option<PathBuf>,
    },

    /// Get file imports
    Imports {
        /// File path
        file: PathBuf,

        /// Resolve imports to their definitions
        #[arg(long)]
        resolve: bool,

        /// Use already running MCP daemon over unix socket
        #[arg(long)]
        remote: Option<PathBuf>,
    },

    /// Show changed symbols (git diff)
    Changed {
        /// Git reference to compare against (default: HEAD)
        #[arg(long, default_value = "HEAD")]
        base: String,

        /// Include staged changes
        #[arg(long)]
        staged: bool,

        /// Include unstaged changes
        #[arg(long)]
        unstaged: bool,

        /// Output format: full, compact, minimal
        #[arg(long, default_value = "full")]
        format: String,
    },

    /// Show index statistics
    Stats {
        /// Use already running MCP daemon over unix socket
        #[arg(long)]
        remote: Option<PathBuf>,
    },

    /// Clear the index
    Clear,

    /// Work with project dependencies
    Deps {
        #[command(subcommand)]
        command: DepsCommands,
    },

    /// Manage tag inference rules
    Tags {
        #[command(subcommand)]
        command: TagsCommands,
    },

    // === Legacy commands (deprecated, hidden) ===
    /// Query the index (deprecated: use symbols, definition, references instead)
    #[command(hide = true)]
    Query {
        #[command(subcommand)]
        query: QueryCommands,
    },
}

/// Legacy query commands (deprecated)
#[derive(Subcommand)]
pub enum QueryCommands {
    /// Search for symbols (deprecated: use 'symbols' command)
    Search {
        query: String,
        #[arg(long, default_value = "20")]
        limit: usize,
        #[arg(long, default_value = "full")]
        format: String,
        #[arg(long)]
        fuzzy: bool,
        #[arg(long, default_value = "0.7")]
        fuzzy_threshold: f64,
    },

    /// Find symbol definition (deprecated: use 'definition' command)
    Definition { name: String },

    /// List functions (deprecated: use 'symbols --kind function')
    Functions {
        #[arg(long, default_value = "100")]
        limit: usize,
        #[arg(long)]
        language: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        pattern: Option<String>,
        #[arg(long, default_value = "full")]
        format: String,
    },

    /// List types (deprecated: use 'symbols --kind type')
    Types {
        #[arg(long, default_value = "100")]
        limit: usize,
        #[arg(long)]
        language: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        pattern: Option<String>,
        #[arg(long, default_value = "full")]
        format: String,
    },

    /// Show changed symbols (deprecated: use 'changed' command)
    Changed {
        #[arg(long, default_value = "HEAD")]
        base: String,
        #[arg(long)]
        staged: bool,
        #[arg(long)]
        unstaged: bool,
        #[arg(long, default_value = "full")]
        format: String,
    },
}

#[derive(Subcommand)]
pub enum DepsCommands {
    /// List project dependencies
    List {
        /// Path to the project directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Include dev dependencies
        #[arg(long)]
        dev: bool,

        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Index dependencies (parse symbols from dependency sources)
    Index {
        /// Path to the project directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Index only specific dependency by name
        #[arg(long)]
        name: Option<String>,

        /// Also index dev dependencies
        #[arg(long)]
        dev: bool,
    },

    /// Find a symbol in dependencies
    Find {
        /// Symbol name to search
        name: String,

        /// Filter by dependency name
        #[arg(long)]
        dep: Option<String>,

        /// Maximum number of results
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Get source code for a symbol from dependency
    Source {
        /// Symbol name
        name: String,

        /// Filter by dependency name
        #[arg(long)]
        dep: Option<String>,

        /// Number of context lines around the symbol
        #[arg(long, default_value = "10")]
        context: usize,
    },

    /// Show information about a dependency
    Info {
        /// Dependency name
        name: String,

        /// Path to the project directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum TagsCommands {
    /// Add a tag inference rule
    AddRule {
        /// Tag to add (e.g., "domain:auth")
        tag: String,

        /// Glob pattern to match files (e.g., "**/auth/**")
        #[arg(long)]
        pattern: String,

        /// Confidence score (0.0-1.0)
        #[arg(long, default_value = "0.7")]
        confidence: f64,

        /// Path to project root
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Remove a tag inference rule
    RemoveRule {
        /// Glob pattern of the rule to remove
        #[arg(long)]
        pattern: String,

        /// Path to project root
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// List all tag inference rules
    ListRules {
        /// Path to project root
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Preview what tags would be inferred for a file
    Preview {
        /// File path to preview
        file: PathBuf,

        /// Path to project root
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Apply tag rules to the index
    Apply {
        /// Path to project root
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Database path
        #[arg(long, default_value = ".code-index.db")]
        db: PathBuf,
    },

    /// Show tag statistics
    Stats {
        /// Database path
        #[arg(long, default_value = ".code-index.db")]
        db: PathBuf,
    },
}

pub fn index_directory(
    path: &Path,
    db_path: &Path,
    watch: bool,
    durability: IndexDurability,
) -> Result<()> {
    use code_indexer::indexer::ExtractionResult;

    #[derive(Clone)]
    struct FileWorkItem {
        path: PathBuf,
        content_hash: String,
    }

    // If db_path is the default value, place the database inside the indexed directory
    let effective_db = if db_path == Path::new(".code-index.db") {
        path.join(".code-index.db")
    } else {
        db_path.to_path_buf()
    };

    let registry = LanguageRegistry::new();
    let walker = FileWalker::new(registry);
    let index = SqliteIndex::new(&effective_db)?;
    let use_fast_bulk = durability == IndexDurability::Fast && !watch;

    if durability == IndexDurability::Fast && watch {
        eprintln!("--durability fast is ignored in watch mode; using safe durability for updates.");
    }

    let files = walker.walk(path)?;
    let current_files: HashSet<String> = files
        .iter()
        .map(|file| file.to_string_lossy().to_string())
        .collect();

    let tracked_files = index.get_tracked_files()?;
    let stale_files: Vec<String> = tracked_files
        .into_iter()
        .filter(|tracked| !current_files.contains(tracked))
        .collect();
    if !stale_files.is_empty() {
        let stale_refs: Vec<&str> = stale_files.iter().map(|p| p.as_str()).collect();
        index.remove_files_batch(&stale_refs)?;
        println!("Removed {} stale files from index", stale_files.len());
    }

    // === Phase 1: Process sidecar files ===
    let sidecar_meta = process_sidecar_files(&files, path, &index)?;

    let mut files_to_index = Vec::new();
    for file in &files {
        match fs::read_to_string(file) {
            Ok(content) => {
                let file_path = file.to_string_lossy().to_string();
                let content_hash = SqliteIndex::compute_content_hash(&content);
                if index.file_needs_reindex(&file_path, &content_hash)? {
                    files_to_index.push(FileWorkItem {
                        path: file.clone(),
                        content_hash,
                    });
                }
            }
            Err(e) => {
                eprintln!("Error reading {}: {}", file.display(), e);
            }
        }
    }

    let unchanged_files = files.len().saturating_sub(files_to_index.len());
    if unchanged_files > 0 {
        println!("Skipping {} unchanged files", unchanged_files);
    }

    // Set up progress tracking
    use crate::indexer::IndexingProgress;
    use indicatif::{ProgressBar, ProgressStyle};

    let progress = IndexingProgress::new();
    progress.start(files_to_index.len());

    let pb = ProgressBar::new(files_to_index.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) | {elapsed_precise} ETA {eta} | {msg}",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    pb.set_message("indexing...");

    // Parallel parsing and extraction using rayon
    let progress_ref = &progress;
    let pb_ref = &pb;
    let total_files = files_to_index.len();
    let files_done = AtomicUsize::new(0);
    let progress_batch_size = 32usize;
    let results: Vec<(IndexedFileRecord, ExtractionResult)> = files_to_index
        .par_iter()
        .map_init(
            || {
                (
                    CodeParser::new(LanguageRegistry::new()),
                    SymbolExtractor::new(),
                )
            },
            |(parser, extractor), file| match parser.parse_file(&file.path) {
                Ok(parsed) => match extractor.extract_all(&parsed, &file.path) {
                    Ok(result) => {
                        let symbol_count = result.symbols.len();
                        let file_record = IndexedFileRecord {
                            path: file.path.to_string_lossy().to_string(),
                            language: parsed.language.clone(),
                            symbol_count,
                            content_hash: file.content_hash.clone(),
                        };
                        progress_ref.inc(symbol_count);
                        let completed = files_done.fetch_add(1, Ordering::Relaxed) + 1;
                        if completed % progress_batch_size == 0 || completed == total_files {
                            pb_ref.set_position(completed as u64);
                        }
                        Some((file_record, result))
                    }
                    Err(e) => {
                        eprintln!(
                            "Error extracting symbols from {}: {}",
                            file.path.display(),
                            e
                        );
                        progress_ref.inc_error();
                        let completed = files_done.fetch_add(1, Ordering::Relaxed) + 1;
                        if completed % progress_batch_size == 0 || completed == total_files {
                            pb_ref.set_position(completed as u64);
                        }
                        None
                    }
                },
                Err(e) => {
                    eprintln!("Error parsing {}: {}", file.path.display(), e);
                    progress_ref.inc_error();
                    let completed = files_done.fetch_add(1, Ordering::Relaxed) + 1;
                    if completed % progress_batch_size == 0 || completed == total_files {
                        pb_ref.set_position(completed as u64);
                    }
                    None
                }
            },
        )
        .filter_map(|r| r)
        .collect();

    let mut file_records = Vec::with_capacity(results.len());
    let mut extraction_results = Vec::with_capacity(results.len());
    for (file_record, extraction_result) in results {
        file_records.push(file_record);
        extraction_results.push(extraction_result);
    }

    // === Phase 2: Compute exported_hash for staleness detection ===
    // Do this before batch insert to avoid moving results
    update_exported_hashes(&extraction_results, &sidecar_meta, &index)?;

    // Remove stale rows for changed files before batch insert.
    let changed_paths: Vec<&str> = file_records.iter().map(|r| r.path.as_str()).collect();
    index.remove_files_batch(&changed_paths)?;

    // Batch insert all changed results.
    let total_symbols =
        index.add_extraction_results_batch_with_durability(extraction_results, use_fast_bulk)?;
    index.upsert_file_records_batch(&file_records)?;

    pb.finish_with_message(format!(
        "{} symbols from {} changed files",
        total_symbols,
        file_records.len()
    ));
    progress.finish();

    if !sidecar_meta.is_empty() {
        println!(
            "Processed {} files with sidecar metadata",
            sidecar_meta.len()
        );
    }

    if watch {
        println!("Watching for changes...");
        let watcher = FileWatcher::new(path)?;
        let walker = FileWalker::new(LanguageRegistry::new());
        let parser = CodeParser::new(LanguageRegistry::new());
        let parse_cache = ParseCache::new();
        let extractor = SymbolExtractor::new();

        loop {
            if let Some(events) = watcher.recv() {
                for event in events {
                    match event {
                        FileEvent::Modified(file_path) | FileEvent::Created(file_path) => {
                            // Handle sidecar file changes
                            if file_path
                                .file_name()
                                .map_or(false, |n| n == SIDECAR_FILENAME)
                            {
                                if let Err(e) =
                                    handle_sidecar_change(&file_path, path, &index, &walker)
                                {
                                    eprintln!("Error processing sidecar change: {}", e);
                                }
                                continue;
                            }

                            // Handle source file changes
                            if walker.is_supported(&file_path) {
                                index.remove_file(&file_path.to_string_lossy())?;
                                if let Ok(parsed) = parse_cache.parse_file(&file_path, &parser) {
                                    if let Ok(result) = extractor.extract_all(&parsed, &file_path) {
                                        let count = result.symbols.len();
                                        index.add_extraction_results_batch(vec![result])?;
                                        let content_hash =
                                            SqliteIndex::compute_content_hash(&parsed.source);
                                        let file_record = IndexedFileRecord {
                                            path: file_path.to_string_lossy().to_string(),
                                            language: parsed.language.clone(),
                                            symbol_count: count,
                                            content_hash,
                                        };
                                        index.upsert_file_records_batch(&[file_record])?;

                                        // Re-apply sidecar metadata for this file
                                        if let Err(e) =
                                            update_file_sidecar_meta(&file_path, path, &index)
                                        {
                                            eprintln!(
                                                "Warning: Could not update sidecar metadata: {}",
                                                e
                                            );
                                        }

                                        println!(
                                            "Updated {}: {} symbols",
                                            file_path.display(),
                                            count
                                        );
                                    }
                                } else {
                                    parse_cache.invalidate(&file_path);
                                }
                            }
                        }
                        FileEvent::Deleted(file_path) => {
                            parse_cache.invalidate(&file_path);
                            if file_path
                                .file_name()
                                .map_or(false, |n| n == SIDECAR_FILENAME)
                            {
                                // Sidecar deleted - remove metadata for files in that directory
                                if let Some(dir) = file_path.parent() {
                                    println!("Sidecar deleted in {}", dir.display());
                                    // Note: we don't remove file_meta as it may have been inferred
                                }
                            } else {
                                index.remove_file(&file_path.to_string_lossy())?;
                                println!("Removed {}", file_path.display());
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub async fn run_mcp_server(
    db_path: &Path,
    transport: ServeTransport,
    socket: Option<&Path>,
) -> Result<()> {
    use crate::mcp::McpServer;
    use rmcp::ServiceExt;

    let index = Arc::new(SqliteIndex::new(db_path)?);

    match transport {
        ServeTransport::Stdio => {
            let server = McpServer::new(index);
            let io_transport = (tokio::io::stdin(), tokio::io::stdout());
            let running = server
                .serve(io_transport)
                .await
                .map_err(|e| crate::error::IndexerError::Mcp(e.to_string()))?;
            running
                .waiting()
                .await
                .map_err(|e| crate::error::IndexerError::Mcp(e.to_string()))?;
            Ok(())
        }
        ServeTransport::Unix => {
            let socket_path = socket.ok_or_else(|| {
                crate::error::IndexerError::Mcp(
                    "--socket is required when --transport unix".to_string(),
                )
            })?;

            if socket_path.exists() {
                let _ = std::fs::remove_file(socket_path);
            }

            let listener = tokio::net::UnixListener::bind(socket_path).map_err(|e| {
                crate::error::IndexerError::Mcp(format!(
                    "Failed to bind unix socket {}: {}",
                    socket_path.display(),
                    e
                ))
            })?;

            println!(
                "MCP daemon listening on unix socket {}",
                socket_path.display()
            );

            loop {
                let (stream, _) = listener.accept().await.map_err(|e| {
                    crate::error::IndexerError::Mcp(format!("Unix accept failed: {}", e))
                })?;
                let server = McpServer::new(index.clone());
                tokio::spawn(async move {
                    use rmcp::ServiceExt;
                    match server.serve(stream).await {
                        Ok(running) => {
                            let _ = running.waiting().await;
                        }
                        Err(e) => {
                            eprintln!("MCP unix client session failed: {}", e);
                        }
                    }
                });
            }
        }
    }
}

fn open_query_index(db_path: &Path) -> Result<SqliteIndex> {
    SqliteIndex::new_read_only(db_path)
}

fn tool_result_text(result: rmcp::model::CallToolResult) -> Result<String> {
    let rmcp::model::CallToolResult {
        content,
        structured_content,
        is_error,
        ..
    } = result;

    let mut chunks = Vec::new();
    for content in content {
        if let Some(text) = content.raw.as_text() {
            chunks.push(text.text.clone());
        } else if let Some(resource) = content.raw.as_resource() {
            if let rmcp::model::ResourceContents::TextResourceContents { text, .. } =
                &resource.resource
            {
                chunks.push(text.clone());
            }
        }
    }

    let fallback = structured_content
        .as_ref()
        .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
        .unwrap_or_default();

    let text = if chunks.is_empty() {
        fallback
    } else {
        chunks.join("\n")
    };

    if is_error.unwrap_or(false) {
        return Err(crate::error::IndexerError::Mcp(if text.is_empty() {
            "Remote MCP tool returned an error".to_string()
        } else {
            text
        }));
    }

    Ok(text)
}

async fn call_remote_tool(
    socket_path: &Path,
    tool_name: &str,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> Result<String> {
    use rmcp::ServiceExt;

    let stream = tokio::net::UnixStream::connect(socket_path)
        .await
        .map_err(|e| {
            crate::error::IndexerError::Mcp(format!(
                "Failed to connect remote daemon {}: {}",
                socket_path.display(),
                e
            ))
        })?;

    let mut client = ()
        .serve(stream)
        .await
        .map_err(|e| crate::error::IndexerError::Mcp(e.to_string()))?;

    let result = client
        .peer()
        .call_tool(rmcp::model::CallToolRequestParams {
            name: tool_name.to_string().into(),
            arguments: Some(arguments),
            meta: None,
            task: None,
        })
        .await
        .map_err(|e| crate::error::IndexerError::Mcp(e.to_string()))?;

    let output = tool_result_text(result);
    let _ = client.close().await;
    output
}

pub fn search_symbols(
    db_path: &Path,
    query: &str,
    limit: usize,
    format: &str,
    fuzzy: bool,
    fuzzy_threshold: f64,
) -> Result<()> {
    let index = open_query_index(db_path)?;
    let output_format = OutputFormat::from_str(format).unwrap_or(OutputFormat::Full);
    let options = SearchOptions {
        limit: Some(limit),
        output_format: Some(output_format),
        fuzzy: Some(fuzzy),
        fuzzy_threshold: Some(fuzzy_threshold),
        ..Default::default()
    };

    let results = if fuzzy {
        index.search_fuzzy(query, &options)?
    } else {
        index.search(query, &options)?
    };

    if results.is_empty() {
        println!("No symbols found for query: {}", query);
        return Ok(());
    }

    match output_format {
        OutputFormat::Full => {
            for result in results {
                let symbol = result.symbol;
                let kind = symbol.kind.as_str();
                let location = format!(
                    "{}:{}",
                    symbol.location.file_path, symbol.location.start_line
                );
                println!(
                    "{} ({}) - {} [score: {:.2}]",
                    symbol.name, kind, location, result.score
                );
            }
        }
        OutputFormat::Compact => {
            let compact: Vec<CompactSymbol> = results
                .iter()
                .map(|r| CompactSymbol::from_symbol(&r.symbol, Some(r.score)))
                .collect();
            println!("{}", serde_json::to_string(&compact).unwrap_or_default());
        }
        OutputFormat::Minimal => {
            let lines: Vec<String> = results
                .iter()
                .map(|r| CompactSymbol::from_symbol(&r.symbol, Some(r.score)).to_minimal_string())
                .collect();
            println!("{}", lines.join(", "));
        }
    }

    Ok(())
}

pub async fn find_definition(
    db_path: &Path,
    name: &str,
    include_deps: bool,
    dep: Option<String>,
    remote: Option<&Path>,
) -> Result<()> {
    if let Some(socket) = remote {
        let mut args = serde_json::Map::new();
        args.insert(
            "name".to_string(),
            serde_json::Value::String(name.to_string()),
        );
        if include_deps {
            args.insert("include_deps".to_string(), serde_json::Value::Bool(true));
        }
        if let Some(dep_name) = dep {
            args.insert(
                "dependency".to_string(),
                serde_json::Value::String(dep_name),
            );
        }
        let output = call_remote_tool(socket, "find_definitions", args).await?;
        if !output.is_empty() {
            println!("{}", output);
        }
        return Ok(());
    }

    if include_deps {
        return find_in_dependencies(db_path, name, dep, 20);
    }

    let index = open_query_index(db_path)?;
    let symbols = index.find_definition(name)?;

    if symbols.is_empty() {
        println!("No definition found for: {}", name);
        return Ok(());
    }

    for symbol in symbols {
        let kind = symbol.kind.as_str();
        let location = format!(
            "{}:{}",
            symbol.location.file_path, symbol.location.start_line
        );
        println!("{} ({}) - {}", symbol.name, kind, location);
        if let Some(sig) = &symbol.signature {
            println!("  Signature: {}", sig);
        }
    }

    Ok(())
}

pub fn list_functions(
    db_path: &Path,
    limit: usize,
    language: Option<String>,
    file: Option<String>,
    pattern: Option<String>,
    format: &str,
) -> Result<()> {
    let index = open_query_index(db_path)?;
    let output_format = OutputFormat::from_str(format).unwrap_or(OutputFormat::Full);
    let options = SearchOptions {
        limit: Some(limit),
        language_filter: language.map(|l| vec![l]),
        file_filter: file,
        name_filter: pattern,
        output_format: Some(output_format),
        ..Default::default()
    };

    let symbols = index.list_functions(&options)?;

    if symbols.is_empty() {
        println!("No functions found");
        return Ok(());
    }

    match output_format {
        OutputFormat::Full => {
            for symbol in symbols {
                let kind = symbol.kind.as_str();
                let location = format!(
                    "{}:{}",
                    symbol.location.file_path, symbol.location.start_line
                );
                println!("{} ({}) - {}", symbol.name, kind, location);
            }
        }
        OutputFormat::Compact => {
            let compact: Vec<CompactSymbol> = symbols
                .iter()
                .map(|s| CompactSymbol::from_symbol(s, None))
                .collect();
            println!("{}", serde_json::to_string(&compact).unwrap_or_default());
        }
        OutputFormat::Minimal => {
            let lines: Vec<String> = symbols
                .iter()
                .map(|s| CompactSymbol::from_symbol(s, None).to_minimal_string())
                .collect();
            println!("{}", lines.join(", "));
        }
    }

    Ok(())
}

pub fn list_types(
    db_path: &Path,
    limit: usize,
    language: Option<String>,
    file: Option<String>,
    pattern: Option<String>,
    format: &str,
) -> Result<()> {
    let index = open_query_index(db_path)?;
    let output_format = OutputFormat::from_str(format).unwrap_or(OutputFormat::Full);
    let options = SearchOptions {
        limit: Some(limit),
        language_filter: language.map(|l| vec![l]),
        file_filter: file,
        name_filter: pattern,
        output_format: Some(output_format),
        ..Default::default()
    };

    let symbols = index.list_types(&options)?;

    if symbols.is_empty() {
        println!("No types found");
        return Ok(());
    }

    match output_format {
        OutputFormat::Full => {
            for symbol in symbols {
                let kind = symbol.kind.as_str();
                let location = format!(
                    "{}:{}",
                    symbol.location.file_path, symbol.location.start_line
                );
                println!("{} ({}) - {}", symbol.name, kind, location);
            }
        }
        OutputFormat::Compact => {
            let compact: Vec<CompactSymbol> = symbols
                .iter()
                .map(|s| CompactSymbol::from_symbol(s, None))
                .collect();
            println!("{}", serde_json::to_string(&compact).unwrap_or_default());
        }
        OutputFormat::Minimal => {
            let lines: Vec<String> = symbols
                .iter()
                .map(|s| CompactSymbol::from_symbol(s, None).to_minimal_string())
                .collect();
            println!("{}", lines.join(", "));
        }
    }

    Ok(())
}

pub async fn show_stats(db_path: &Path, remote: Option<&Path>) -> Result<()> {
    if let Some(socket) = remote {
        let output = call_remote_tool(socket, "get_stats", serde_json::Map::new()).await?;
        if !output.is_empty() {
            println!("{}", output);
        }
        return Ok(());
    }

    let index = open_query_index(db_path)?;
    let stats = index.get_stats()?;

    println!("Index Statistics:");
    println!("  Total files: {}", stats.total_files);
    println!("  Total symbols: {}", stats.total_symbols);

    if !stats.symbols_by_kind.is_empty() {
        println!("\n  Symbols by kind:");
        for (kind, count) in &stats.symbols_by_kind {
            println!("    {}: {}", kind, count);
        }
    }

    if !stats.symbols_by_language.is_empty() {
        println!("\n  Symbols by language:");
        for (lang, count) in &stats.symbols_by_language {
            println!("    {}: {}", lang, count);
        }
    }

    if !stats.files_by_language.is_empty() {
        println!("\n  Files by language:");
        for (lang, count) in &stats.files_by_language {
            println!("    {}: {}", lang, count);
        }
    }

    Ok(())
}

pub fn clear_index(db_path: &Path) -> Result<()> {
    let index = open_query_index(db_path)?;
    index.clear()?;
    println!("Index cleared");
    Ok(())
}

// === New consolidated commands ===

/// Unified symbols command (replaces search, list_functions, list_types)
pub async fn symbols(
    db_path: &Path,
    query: Option<String>,
    kind: &str,
    limit: usize,
    language: Option<String>,
    file: Option<String>,
    pattern: Option<String>,
    format: &str,
    fuzzy: bool,
    fuzzy_threshold: f64,
    remote: Option<&Path>,
) -> Result<()> {
    if let Some(socket) = remote {
        let mut args = serde_json::Map::new();
        args.insert(
            "kind".to_string(),
            serde_json::Value::String(kind.to_string()),
        );
        args.insert("limit".to_string(), serde_json::Value::from(limit));
        args.insert(
            "format".to_string(),
            serde_json::Value::String(format.to_string()),
        );
        if let Some(lang) = language {
            args.insert("language".to_string(), serde_json::Value::String(lang));
        }
        if let Some(file_filter) = file {
            args.insert("file".to_string(), serde_json::Value::String(file_filter));
        }
        if let Some(name_pattern) = pattern {
            args.insert(
                "pattern".to_string(),
                serde_json::Value::String(name_pattern),
            );
        }

        let (tool_name, final_args) = if let Some(q) = query {
            args.insert("query".to_string(), serde_json::Value::String(q));
            args.insert("fuzzy".to_string(), serde_json::Value::Bool(fuzzy));
            args.insert(
                "fuzzy_threshold".to_string(),
                serde_json::Value::from(fuzzy_threshold),
            );
            ("search_symbols", args)
        } else {
            ("list_symbols", args)
        };

        let output = call_remote_tool(socket, tool_name, final_args).await?;
        if !output.is_empty() {
            println!("{}", output);
        }
        return Ok(());
    }

    let index = open_query_index(db_path)?;
    let output_format = OutputFormat::from_str(format).unwrap_or(OutputFormat::Full);

    // If query is provided, search; otherwise list
    if let Some(ref q) = query {
        let kind_filter = match kind {
            "function" | "functions" => Some(vec![crate::index::SymbolKind::Function]),
            "type" | "types" => Some(vec![
                crate::index::SymbolKind::Class,
                crate::index::SymbolKind::Struct,
                crate::index::SymbolKind::Enum,
                crate::index::SymbolKind::Interface,
                crate::index::SymbolKind::TypeAlias,
            ]),
            _ => None,
        };

        let options = SearchOptions {
            limit: Some(limit),
            kind_filter,
            language_filter: language.map(|l| vec![l]),
            file_filter: file,
            name_filter: pattern,
            output_format: Some(output_format),
            fuzzy: Some(fuzzy),
            fuzzy_threshold: Some(fuzzy_threshold),
            ..Default::default()
        };

        let results = if fuzzy {
            index.search_fuzzy(q, &options)?
        } else {
            index.search(q, &options)?
        };

        if results.is_empty() {
            println!("No symbols found for query: {}", q);
            return Ok(());
        }

        print_search_results(&results, output_format);
    } else {
        // List mode
        let options = SearchOptions {
            limit: Some(limit),
            language_filter: language.map(|l| vec![l]),
            file_filter: file,
            name_filter: pattern,
            output_format: Some(output_format),
            ..Default::default()
        };

        let symbols = match kind {
            "function" | "functions" => index.list_functions(&options)?,
            "type" | "types" => index.list_types(&options)?,
            _ => {
                let mut all = index.list_functions(&options).unwrap_or_default();
                all.extend(index.list_types(&options).unwrap_or_default());
                all.truncate(limit);
                all
            }
        };

        if symbols.is_empty() {
            println!("No symbols found");
            return Ok(());
        }

        print_symbols(&symbols, output_format);
    }

    Ok(())
}

/// Find references command
pub async fn find_references(
    db_path: &Path,
    name: &str,
    include_callers: bool,
    depth: u32,
    file: Option<String>,
    limit: usize,
    remote: Option<&Path>,
) -> Result<()> {
    if let Some(socket) = remote {
        let mut args = serde_json::Map::new();
        args.insert(
            "name".to_string(),
            serde_json::Value::String(name.to_string()),
        );
        args.insert(
            "include_callers".to_string(),
            serde_json::Value::Bool(include_callers),
        );
        args.insert("depth".to_string(), serde_json::Value::from(depth));
        args.insert("limit".to_string(), serde_json::Value::from(limit));
        if let Some(file_filter) = file {
            args.insert("file".to_string(), serde_json::Value::String(file_filter));
        }
        let output = call_remote_tool(socket, "find_references", args).await?;
        if !output.is_empty() {
            println!("{}", output);
        }
        return Ok(());
    }

    let index = open_query_index(db_path)?;

    let options = SearchOptions {
        limit: Some(limit),
        file_filter: file,
        ..Default::default()
    };

    // Get basic references
    let refs = index.find_references(name, &options)?;

    println!("References for '{}':", name);
    if refs.is_empty() {
        println!("  No references found");
    } else {
        for r in &refs {
            println!(
                "  {} ({:?}) at {}:{}",
                r.symbol_name, r.kind, r.file_path, r.line
            );
        }
    }

    // Include callers if requested
    if include_callers {
        println!("\nCallers:");
        match index.find_callers(name, Some(depth)) {
            Ok(callers) => {
                if callers.is_empty() {
                    println!("  No callers found");
                } else {
                    for c in callers {
                        println!("  {} at {}:{}", c.symbol_name, c.file_path, c.line);
                    }
                }
            }
            Err(e) => println!("  Error finding callers: {}", e),
        }
    }

    Ok(())
}

/// Call graph analysis command
pub async fn analyze_call_graph(
    db_path: &Path,
    function: &str,
    direction: &str,
    depth: u32,
    include_possible: bool,
    remote: Option<&Path>,
) -> Result<()> {
    if let Some(socket) = remote {
        let mut args = serde_json::Map::new();
        args.insert(
            "function".to_string(),
            serde_json::Value::String(function.to_string()),
        );
        args.insert(
            "direction".to_string(),
            serde_json::Value::String(direction.to_string()),
        );
        args.insert("depth".to_string(), serde_json::Value::from(depth));
        args.insert(
            "include_possible".to_string(),
            serde_json::Value::Bool(include_possible),
        );
        let output = call_remote_tool(socket, "analyze_call_graph", args).await?;
        if !output.is_empty() {
            println!("{}", output);
        }
        return Ok(());
    }

    let index = open_query_index(db_path)?;

    println!(
        "Call graph for '{}' (direction: {}, depth: {}):",
        function, direction, depth
    );

    // Outgoing calls (callees)
    if direction == "out" || direction == "both" {
        println!("\nOutgoing calls (callees):");
        match index.get_call_graph(function, depth) {
            Ok(graph) => {
                if graph.edges.is_empty() {
                    println!("  No outgoing calls found");
                } else {
                    for edge in &graph.edges {
                        let target = edge.to.as_deref().unwrap_or(&edge.callee_name);
                        println!(
                            "  {} -> {} at {}:{}",
                            edge.from, target, edge.call_site_file, edge.call_site_line
                        );
                    }
                }
            }
            Err(e) => println!("  Error: {}", e),
        }
    }

    // Incoming calls (callers)
    if direction == "in" || direction == "both" {
        println!("\nIncoming calls (callers):");
        match index.find_callers(function, Some(depth)) {
            Ok(callers) => {
                if callers.is_empty() {
                    println!("  No callers found");
                } else {
                    for c in callers {
                        println!(
                            "  {} <- {} at {}:{}",
                            function, c.symbol_name, c.file_path, c.line
                        );
                    }
                }
            }
            Err(e) => println!("  Error: {}", e),
        }
    }

    Ok(())
}

/// Get file outline command
pub async fn get_outline(
    db_path: &Path,
    file: &Path,
    start_line: Option<u32>,
    end_line: Option<u32>,
    include_scopes: bool,
    remote: Option<&Path>,
) -> Result<()> {
    if let Some(socket) = remote {
        let mut args = serde_json::Map::new();
        args.insert(
            "file".to_string(),
            serde_json::Value::String(file.to_string_lossy().to_string()),
        );
        if let Some(start) = start_line {
            args.insert("start_line".to_string(), serde_json::Value::from(start));
        }
        if let Some(end) = end_line {
            args.insert("end_line".to_string(), serde_json::Value::from(end));
        }
        args.insert(
            "include_scopes".to_string(),
            serde_json::Value::Bool(include_scopes),
        );
        let output = call_remote_tool(socket, "get_file_outline", args).await?;
        if !output.is_empty() {
            println!("{}", output);
        }
        return Ok(());
    }

    let index = open_query_index(db_path)?;
    let file_path = file.to_string_lossy();

    let symbols = index.get_file_symbols(&file_path)?;

    // Filter by line range if specified
    let filtered: Vec<_> = if start_line.is_some() || end_line.is_some() {
        let start = start_line.unwrap_or(0);
        let end = end_line.unwrap_or(u32::MAX);
        symbols
            .into_iter()
            .filter(|s| s.location.start_line >= start && s.location.end_line <= end)
            .collect()
    } else {
        symbols
    };

    println!("Outline for {}:", file_path);

    if filtered.is_empty() {
        println!("  No symbols found");
    } else {
        for symbol in &filtered {
            let indent = if symbol.parent.is_some() {
                "    "
            } else {
                "  "
            };
            println!(
                "{}{} ({}) - lines {}-{}",
                indent,
                symbol.name,
                symbol.kind.as_str(),
                symbol.location.start_line,
                symbol.location.end_line
            );
        }
    }

    // Include scopes if requested
    if include_scopes {
        println!("\nScopes:");
        match index.get_file_scopes(&file_path) {
            Ok(scopes) => {
                if scopes.is_empty() {
                    println!("  No scopes found");
                } else {
                    for scope in scopes {
                        let name = scope.name.as_deref().unwrap_or("<anonymous>");
                        println!(
                            "  {} ({}) - lines {}-{}",
                            name,
                            scope.kind.as_str(),
                            scope.start_line,
                            scope.end_line
                        );
                    }
                }
            }
            Err(e) => println!("  Error: {}", e),
        }
    }

    Ok(())
}

/// Get file imports command
pub async fn get_imports(
    db_path: &Path,
    file: &Path,
    resolve: bool,
    remote: Option<&Path>,
) -> Result<()> {
    if let Some(socket) = remote {
        let mut args = serde_json::Map::new();
        args.insert(
            "file".to_string(),
            serde_json::Value::String(file.to_string_lossy().to_string()),
        );
        args.insert("resolve".to_string(), serde_json::Value::Bool(resolve));
        let output = call_remote_tool(socket, "get_imports", args).await?;
        if !output.is_empty() {
            println!("{}", output);
        }
        return Ok(());
    }

    let index = open_query_index(db_path)?;
    let file_path = file.to_string_lossy();

    let imports = index.get_file_imports(&file_path)?;

    println!("Imports for {}:", file_path);

    if imports.is_empty() {
        println!("  No imports found");
        return Ok(());
    }

    for import in &imports {
        let symbol = import.imported_symbol.as_deref().unwrap_or("<module>");
        let path = import.imported_path.as_deref().unwrap_or("");
        println!("  {} from {}", symbol, path);

        // Resolve if requested
        if resolve {
            if let Some(ref symbol_name) = import.imported_symbol {
                match index.find_definition(symbol_name) {
                    Ok(defs) => {
                        for def in defs {
                            println!(
                                "    -> {} at {}:{}",
                                def.name, def.location.file_path, def.location.start_line
                            );
                        }
                    }
                    Err(_) => println!("    -> (could not resolve)"),
                }
            }
        }
    }

    Ok(())
}

// Helper functions for output

fn print_search_results(results: &[crate::index::SearchResult], format: OutputFormat) {
    match format {
        OutputFormat::Full => {
            for result in results {
                let symbol = &result.symbol;
                let kind = symbol.kind.as_str();
                let location = format!(
                    "{}:{}",
                    symbol.location.file_path, symbol.location.start_line
                );
                println!(
                    "{} ({}) - {} [score: {:.2}]",
                    symbol.name, kind, location, result.score
                );
            }
        }
        OutputFormat::Compact => {
            let compact: Vec<CompactSymbol> = results
                .iter()
                .map(|r| CompactSymbol::from_symbol(&r.symbol, Some(r.score)))
                .collect();
            println!("{}", serde_json::to_string(&compact).unwrap_or_default());
        }
        OutputFormat::Minimal => {
            let lines: Vec<String> = results
                .iter()
                .map(|r| CompactSymbol::from_symbol(&r.symbol, Some(r.score)).to_minimal_string())
                .collect();
            println!("{}", lines.join(", "));
        }
    }
}

fn print_symbols(symbols: &[crate::index::Symbol], format: OutputFormat) {
    match format {
        OutputFormat::Full => {
            for symbol in symbols {
                let kind = symbol.kind.as_str();
                let location = format!(
                    "{}:{}",
                    symbol.location.file_path, symbol.location.start_line
                );
                println!("{} ({}) - {}", symbol.name, kind, location);
            }
        }
        OutputFormat::Compact => {
            let compact: Vec<CompactSymbol> = symbols
                .iter()
                .map(|s| CompactSymbol::from_symbol(s, None))
                .collect();
            println!("{}", serde_json::to_string(&compact).unwrap_or_default());
        }
        OutputFormat::Minimal => {
            let lines: Vec<String> = symbols
                .iter()
                .map(|s| CompactSymbol::from_symbol(s, None).to_minimal_string())
                .collect();
            println!("{}", lines.join(", "));
        }
    }
}

// === Dependency Commands ===

/// Lists dependencies for a project.
pub fn list_dependencies(
    path: &Path,
    db_path: &Path,
    include_dev: bool,
    format: &str,
) -> Result<()> {
    let registry = DependencyRegistry::with_defaults();

    // Find manifest file
    let project = find_and_parse_project(path, &registry)?;

    // Store in database first (before potentially moving dependencies)
    let index = SqliteIndex::new(db_path)?;
    let project_id = index.add_project(&project)?;
    index.add_dependencies(project_id, &project.dependencies)?;

    if format == "json" {
        let output = serde_json::to_string_pretty(&project.dependencies).unwrap_or_default();
        println!("{}", output);
    } else {
        println!("Project: {} ({})", project.name, project.ecosystem.as_str());
        if let Some(ref version) = project.version {
            println!("Version: {}", version);
        }
        println!();

        let deps: Vec<_> = if include_dev {
            project.dependencies
        } else {
            project
                .dependencies
                .into_iter()
                .filter(|d| !d.is_dev)
                .collect()
        };

        if deps.is_empty() {
            println!("No dependencies found");
            return Ok(());
        }

        println!("Dependencies ({}):", deps.len());
        for dep in deps {
            let dev_marker = if dep.is_dev { " [dev]" } else { "" };
            let source_status = if dep.source_path.is_some() {
                "sources available"
            } else {
                "no sources"
            };
            println!(
                "  {} @ {} ({}){}",
                dep.name, dep.version, source_status, dev_marker
            );
        }
    }

    Ok(())
}

/// Indexes symbols from dependencies.
pub fn index_dependencies(
    path: &Path,
    db_path: &Path,
    dep_name: Option<String>,
    include_dev: bool,
) -> Result<()> {
    let dep_registry = DependencyRegistry::with_defaults();
    let lang_registry = LanguageRegistry::new();
    let parser = CodeParser::new(lang_registry);
    let extractor = SymbolExtractor::new();

    let project = find_and_parse_project(path, &dep_registry)?;
    let index = SqliteIndex::new(db_path)?;

    // Store project info
    let project_id = index.add_project(&project)?;
    index.add_dependencies(project_id, &project.dependencies)?;

    let deps_to_index: Vec<_> = project
        .dependencies
        .iter()
        .filter(|d| {
            if !include_dev && d.is_dev {
                return false;
            }
            if let Some(ref name) = dep_name {
                return &d.name == name;
            }
            true
        })
        .filter(|d| d.source_path.is_some())
        .collect();

    if deps_to_index.is_empty() {
        println!("No dependencies with available sources to index");
        return Ok(());
    }

    println!("Indexing {} dependencies...", deps_to_index.len());

    let lang_registry = LanguageRegistry::new();
    let walker = FileWalker::new(lang_registry);

    // Collect dep_ids for batch marking at the end
    let mut indexed_dep_ids: Vec<i64> = Vec::new();

    for dep in deps_to_index {
        // Safe: deps_to_index is filtered to only include deps with source_path
        let Some(source_path) = dep.source_path.as_ref() else {
            continue;
        };
        let source_dir = PathBuf::from(source_path);

        if !source_dir.exists() {
            println!("  Skipping {} (source not found)", dep.name);
            continue;
        }

        print!("  Indexing {}...", dep.name);

        let files = match walker.walk(&source_dir) {
            Ok(files) => files,
            Err(e) => {
                println!(" error: {}", e);
                continue;
            }
        };

        let dep_id = match index.get_dependency_id(project_id, &dep.name)? {
            Some(id) => id,
            None => {
                println!(" error: dependency not in database");
                continue;
            }
        };

        let mut total_symbols = 0;
        for file in &files {
            if let Ok(parsed) = parser.parse_file(file) {
                if let Ok(symbols) = extractor.extract(&parsed, file) {
                    let count = symbols.len();
                    if let Err(e) = index.add_dependency_symbols(dep_id, symbols) {
                        eprintln!(" warning: {}", e);
                    } else {
                        total_symbols += count;
                    }
                }
            }
        }

        indexed_dep_ids.push(dep_id);
        println!(" {} symbols from {} files", total_symbols, files.len());
    }

    // Batch mark all successfully indexed dependencies
    if !indexed_dep_ids.is_empty() {
        index.mark_dependencies_indexed_batch(&indexed_dep_ids)?;
    }

    Ok(())
}

/// Finds a symbol in indexed dependencies.
pub fn find_in_dependencies(
    db_path: &Path,
    name: &str,
    dep: Option<String>,
    limit: usize,
) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
    let options = SearchOptions {
        limit: Some(limit),
        ..Default::default()
    };

    let results = index.search_in_dependencies(name, dep.as_deref(), &options)?;

    if results.is_empty() {
        println!("No symbols found for '{}' in dependencies", name);
        return Ok(());
    }

    for result in results {
        let symbol = result.symbol;
        let kind = symbol.kind.as_str();
        let location = format!(
            "{}:{}",
            symbol.location.file_path, symbol.location.start_line
        );
        println!(
            "{} ({}) - {} [score: {:.2}]",
            symbol.name, kind, location, result.score
        );
        if let Some(sig) = &symbol.signature {
            println!("  Signature: {}", sig);
        }
    }

    Ok(())
}

/// Gets the source code for a symbol from a dependency.
pub fn get_dependency_source(
    db_path: &Path,
    name: &str,
    dep: Option<String>,
    context_lines: usize,
) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
    let symbols = index.find_definition_in_dependencies(name, dep.as_deref())?;

    if symbols.is_empty() {
        println!("No definition found for '{}' in dependencies", name);
        return Ok(());
    }

    for symbol in symbols {
        println!("=== {} ({}) ===", symbol.name, symbol.kind.as_str());
        println!("File: {}", symbol.location.file_path);

        // Read the source file
        let file_path = Path::new(&symbol.location.file_path);
        if !file_path.exists() {
            println!("Source file not found: {}", symbol.location.file_path);
            continue;
        }

        let content = fs::read_to_string(file_path)?;
        let lines: Vec<&str> = content.lines().collect();

        let start = symbol.location.start_line.saturating_sub(1) as usize;
        let end = symbol.location.end_line as usize;

        // Calculate context boundaries
        let ctx_start = start.saturating_sub(context_lines);
        let ctx_end = (end + context_lines).min(lines.len());

        println!("Lines {}-{}:", ctx_start + 1, ctx_end);
        println!("---");

        for (i, line) in lines[ctx_start..ctx_end].iter().enumerate() {
            let line_num = ctx_start + i + 1;
            let marker = if line_num >= start + 1 && line_num <= end {
                ">"
            } else {
                " "
            };
            println!("{} {:4} | {}", marker, line_num, line);
        }
        println!("---");
        println!();
    }

    Ok(())
}

/// Shows information about a dependency.
pub fn show_dependency_info(path: &Path, db_path: &Path, name: &str) -> Result<()> {
    let registry = DependencyRegistry::with_defaults();
    let project = find_and_parse_project(path, &registry)?;

    let dep = project
        .dependencies
        .iter()
        .find(|d| d.name == name)
        .ok_or_else(|| {
            crate::error::IndexerError::Index(format!("Dependency '{}' not found", name))
        })?;

    println!("Dependency: {}", dep.name);
    println!("Version: {}", dep.version);
    println!("Ecosystem: {}", dep.ecosystem.as_str());
    println!("Dev dependency: {}", if dep.is_dev { "yes" } else { "no" });

    if let Some(ref source_path) = dep.source_path {
        println!("Source path: {}", source_path);

        // Count files
        let source_dir = PathBuf::from(source_path);
        if source_dir.exists() {
            let lang_registry = LanguageRegistry::new();
            let walker = FileWalker::new(lang_registry);
            if let Ok(files) = walker.walk(&source_dir) {
                println!("Source files: {}", files.len());
            }
        }
    } else {
        println!("Source path: not available");
    }

    // Check if indexed
    let index = SqliteIndex::new(db_path)?;
    if let Some(project_id) = index.get_project_id(&project.manifest_path)? {
        if let Some(db_dep) = index.get_dependency(project_id, name)? {
            println!("Indexed: {}", if db_dep.is_indexed { "yes" } else { "no" });
        }
    }

    Ok(())
}

/// Helper function to find and parse project manifest.
fn find_and_parse_project(path: &Path, registry: &DependencyRegistry) -> Result<ProjectInfo> {
    // Check if path is a manifest file
    if path.is_file() {
        return registry.parse_manifest(path);
    }

    // Otherwise, look for manifest files in the directory
    if let Some(ecosystem) = registry.detect_ecosystem(path) {
        for manifest_name in ecosystem.manifest_names() {
            let manifest_path = path.join(manifest_name);
            if manifest_path.exists() {
                return registry.parse_manifest(&manifest_path);
            }
        }
    }

    Err(crate::error::IndexerError::FileNotFound(
        "No manifest file found in directory".to_string(),
    ))
}

// === Sidecar Processing Functions ===

/// Processes sidecar files (.code-indexer.yml) and stores metadata in the index.
/// Returns a map of file_path -> file_path for files that had sidecar metadata.
fn process_sidecar_files(
    files: &[PathBuf],
    root: &Path,
    index: &SqliteIndex,
) -> Result<HashMap<String, String>> {
    use std::collections::HashSet;

    // Collect unique directories
    let mut directories: HashSet<PathBuf> = HashSet::new();
    for file in files {
        if let Some(parent) = file.parent() {
            directories.insert(parent.to_path_buf());
        }
    }

    // Get tag dictionary for resolution
    let tag_dict = index.get_tag_dictionary().unwrap_or_default();

    let mut files_with_meta: HashMap<String, String> = HashMap::new();
    let mut sidecar_count = 0;

    // Process each directory's sidecar
    for dir in directories {
        let sidecar_path = dir.join(SIDECAR_FILENAME);
        if !sidecar_path.exists() {
            continue;
        }

        let content = match fs::read_to_string(&sidecar_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: Could not read {}: {}", sidecar_path.display(), e);
                continue;
            }
        };

        let sidecar_data = match parse_sidecar(&content) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Warning: Could not parse {}: {}", sidecar_path.display(), e);
                continue;
            }
        };

        sidecar_count += 1;
        let dir_str = dir.to_string_lossy();

        // Process files in this directory that are listed in the sidecar
        for file in files {
            if file.parent() != Some(dir.as_path()) {
                continue;
            }

            let file_path_str = file.to_string_lossy().to_string();

            // Make relative path for sidecar lookup
            let relative_path = file
                .strip_prefix(root)
                .unwrap_or(file)
                .to_string_lossy()
                .to_string();

            // Extract file metadata from sidecar
            if let Some(meta) = extract_file_meta(&relative_path, &sidecar_data, &dir_str) {
                if let Err(e) = index.upsert_file_meta(&meta) {
                    eprintln!(
                        "Warning: Could not save metadata for {}: {}",
                        file_path_str, e
                    );
                } else {
                    files_with_meta.insert(file_path_str.clone(), relative_path.clone());
                }
            }

            // Extract and save file tags
            let tag_strings = extract_file_tags(&relative_path, &sidecar_data);
            if !tag_strings.is_empty() {
                let file_tags = resolve_tags(&file_path_str, &tag_strings, &tag_dict);
                if !file_tags.is_empty() {
                    if let Err(e) = index.add_file_tags(&file_path_str, &file_tags) {
                        eprintln!("Warning: Could not save tags for {}: {}", file_path_str, e);
                    }
                }
            }
        }
    }

    if sidecar_count > 0 {
        println!("Found {} sidecar files", sidecar_count);
    }

    Ok(files_with_meta)
}

/// Handles changes to a .code-indexer.yml sidecar file during watch mode.
/// Re-processes all files in the directory with the new sidecar metadata.
fn handle_sidecar_change(
    sidecar_path: &Path,
    root: &Path,
    index: &SqliteIndex,
    walker: &FileWalker,
) -> Result<()> {
    let dir = match sidecar_path.parent() {
        Some(d) => d,
        None => return Ok(()),
    };

    println!("Sidecar changed: {}", sidecar_path.display());

    // Read and parse the sidecar
    let content = match fs::read_to_string(sidecar_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: Could not read {}: {}", sidecar_path.display(), e);
            return Ok(());
        }
    };

    let sidecar_data = match parse_sidecar(&content) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Warning: Could not parse {}: {}", sidecar_path.display(), e);
            return Ok(());
        }
    };

    // Get tag dictionary for resolution
    let tag_dict = index.get_tag_dictionary().unwrap_or_default();
    let dir_str = dir.to_string_lossy().to_string();

    // Find all supported files in this directory
    let files: Vec<_> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file() && walker.is_supported(p))
            .collect(),
        Err(e) => {
            eprintln!("Warning: Could not read directory {}: {}", dir.display(), e);
            return Ok(());
        }
    };

    let mut updated_count = 0;

    for file in files {
        let file_path_str = file.to_string_lossy().to_string();
        let relative_path = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .to_string_lossy()
            .to_string();

        // Update file metadata
        if let Some(meta) = extract_file_meta(&relative_path, &sidecar_data, &dir_str) {
            if index.upsert_file_meta(&meta).is_ok() {
                updated_count += 1;
            }
        }

        // Update file tags
        let tag_strings = extract_file_tags(&relative_path, &sidecar_data);
        if !tag_strings.is_empty() {
            let file_tags = resolve_tags(&file_path_str, &tag_strings, &tag_dict);
            if !file_tags.is_empty() {
                let _ = index.add_file_tags(&file_path_str, &file_tags);
            }
        }
    }

    println!("Updated metadata for {} files", updated_count);
    Ok(())
}

/// Updates sidecar metadata for a single file during watch mode.
fn update_file_sidecar_meta(file_path: &Path, root: &Path, index: &SqliteIndex) -> Result<()> {
    let dir = match file_path.parent() {
        Some(d) => d,
        None => return Ok(()),
    };

    let sidecar_path = dir.join(SIDECAR_FILENAME);
    if !sidecar_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&sidecar_path)?;
    let sidecar_data = parse_sidecar(&content)?;

    let tag_dict = index.get_tag_dictionary().unwrap_or_default();
    let dir_str = dir.to_string_lossy().to_string();
    let file_path_str = file_path.to_string_lossy().to_string();
    let relative_path = file_path
        .strip_prefix(root)
        .unwrap_or(file_path)
        .to_string_lossy()
        .to_string();

    // Update file metadata
    if let Some(meta) = extract_file_meta(&relative_path, &sidecar_data, &dir_str) {
        index.upsert_file_meta(&meta)?;
    }

    // Update file tags
    let tag_strings = extract_file_tags(&relative_path, &sidecar_data);
    if !tag_strings.is_empty() {
        let file_tags = resolve_tags(&file_path_str, &tag_strings, &tag_dict);
        if !file_tags.is_empty() {
            index.add_file_tags(&file_path_str, &file_tags)?;
        }
    }

    Ok(())
}

/// Updates exported_hash for files based on their public symbols.
fn update_exported_hashes(
    results: &[code_indexer::indexer::ExtractionResult],
    files_with_meta: &HashMap<String, String>,
    index: &SqliteIndex,
) -> Result<()> {
    for result in results {
        // Get file_path from first symbol's location
        let file_path = match result.symbols.first() {
            Some(symbol) => symbol.location.file_path.clone(),
            None => continue, // Skip files with no symbols
        };

        // Compute exported hash from public symbols
        let exported_hash = compute_exported_hash(&result.symbols);

        // Get existing file_meta or create new one
        let mut meta = index
            .get_file_meta(&file_path)?
            .unwrap_or_else(|| FileMeta::new(&file_path));

        // Update exported_hash
        meta.exported_hash = Some(exported_hash);

        // If this file wasn't in a sidecar, mark as inferred
        if !files_with_meta.contains_key(&file_path) {
            meta.source = MetaSource::Inferred;
            meta.confidence = 0.5;
        }

        // Save updated metadata
        index.upsert_file_meta(&meta)?;
    }

    Ok(())
}

/// Gets symbols from changed files (git diff)
pub fn get_changed_symbols(
    db_path: &Path,
    base: &str,
    staged: bool,
    unstaged: bool,
    format: &str,
) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
    let output_format = OutputFormat::from_str(format).unwrap_or(OutputFormat::Full);

    // Use current directory as repo path (db_path is usually .code-index.db in the repo)
    let repo_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let git = GitAnalyzer::new(repo_path)?;

    // If neither staged nor unstaged is specified, show all uncommitted changes
    let (include_staged, include_unstaged) = if !staged && !unstaged {
        (true, true)
    } else {
        (staged, unstaged)
    };

    let changed_symbols =
        git.find_changed_symbols(&index, base, include_staged, include_unstaged)?;

    if changed_symbols.is_empty() {
        println!("No changed symbols found");
        return Ok(());
    }

    match output_format {
        OutputFormat::Full => {
            println!("Changed symbols ({}):", changed_symbols.len());
            for cs in changed_symbols {
                let kind = cs.symbol.kind.as_str();
                let location = format!(
                    "{}:{}",
                    cs.symbol.location.file_path, cs.symbol.location.start_line
                );
                println!(
                    "  {} ({}) - {} [{}]",
                    cs.symbol.name, kind, location, cs.file_status
                );
            }
        }
        OutputFormat::Compact => {
            let compact: Vec<serde_json::Value> = changed_symbols
                .iter()
                .map(|cs| {
                    serde_json::json!({
                        "n": cs.symbol.name,
                        "k": cs.symbol.kind.short_str(),
                        "f": cs.symbol.location.file_path,
                        "l": cs.symbol.location.start_line,
                        "st": cs.file_status
                    })
                })
                .collect();
            println!("{}", serde_json::to_string(&compact).unwrap_or_default());
        }
        OutputFormat::Minimal => {
            let lines: Vec<String> = changed_symbols
                .iter()
                .map(|cs| {
                    format!(
                        "{}:{}@{}:{} [{}]",
                        cs.symbol.name,
                        cs.symbol.kind.short_str(),
                        cs.symbol.location.file_path,
                        cs.symbol.location.start_line,
                        cs.file_status
                    )
                })
                .collect();
            println!("{}", lines.join(", "));
        }
    }

    Ok(())
}

// === Tags Commands Implementation ===

use crate::indexer::{
    apply_tag_rules, preview_tag_rules, resolve_inferred_tags, RootSidecarData, TagRule,
};

/// Adds a tag inference rule to the root .code-indexer.yml
pub fn add_tag_rule(path: &Path, tag: &str, pattern: &str, confidence: f64) -> Result<()> {
    let sidecar_path = path.join(SIDECAR_FILENAME);

    // Load existing or create new
    let mut data = if sidecar_path.exists() {
        let content = fs::read_to_string(&sidecar_path)?;
        RootSidecarData::parse(&content)?
    } else {
        RootSidecarData::default()
    };

    // Check if pattern already exists
    if let Some(existing) = data.tag_rules.iter_mut().find(|r| r.pattern == pattern) {
        // Update existing rule - add tag if not present
        if !existing.tags.contains(&tag.to_string()) {
            existing.tags.push(tag.to_string());
        }
        existing.confidence = confidence;
        println!("Updated existing rule for pattern '{}'", pattern);
    } else {
        // Add new rule
        data.tag_rules.push(TagRule {
            pattern: pattern.to_string(),
            tags: vec![tag.to_string()],
            confidence,
        });
        println!(
            "Added new tag rule: {} -> {} (confidence: {})",
            pattern, tag, confidence
        );
    }

    // Write back
    let content = serde_yaml::to_string(&data).map_err(|e| {
        crate::error::IndexerError::Parse(format!("Failed to serialize YAML: {}", e))
    })?;
    fs::write(&sidecar_path, content)?;

    println!("Saved to {}", sidecar_path.display());
    Ok(())
}

/// Removes a tag inference rule from the root .code-indexer.yml
pub fn remove_tag_rule(path: &Path, pattern: &str) -> Result<()> {
    let sidecar_path = path.join(SIDECAR_FILENAME);

    if !sidecar_path.exists() {
        println!("No .code-indexer.yml found");
        return Ok(());
    }

    let content = fs::read_to_string(&sidecar_path)?;
    let mut data = RootSidecarData::parse(&content)?;

    let original_len = data.tag_rules.len();
    data.tag_rules.retain(|r| r.pattern != pattern);

    if data.tag_rules.len() < original_len {
        let content = serde_yaml::to_string(&data).map_err(|e| {
            crate::error::IndexerError::Parse(format!("Failed to serialize YAML: {}", e))
        })?;
        fs::write(&sidecar_path, content)?;
        println!("Removed rule with pattern '{}'", pattern);
    } else {
        println!("No rule found with pattern '{}'", pattern);
    }

    Ok(())
}

/// Lists all tag inference rules
pub fn list_tag_rules(path: &Path, format: &str) -> Result<()> {
    let sidecar_path = path.join(SIDECAR_FILENAME);

    if !sidecar_path.exists() {
        println!("No .code-indexer.yml found");
        return Ok(());
    }

    let content = fs::read_to_string(&sidecar_path)?;
    let data = RootSidecarData::parse(&content)?;

    if data.tag_rules.is_empty() {
        println!("No tag rules defined");
        return Ok(());
    }

    if format == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&data.tag_rules).unwrap_or_default()
        );
    } else {
        println!("Tag Rules ({}):\n", data.tag_rules.len());
        for rule in &data.tag_rules {
            println!("  Pattern: {}", rule.pattern);
            println!("  Tags: {}", rule.tags.join(", "));
            println!("  Confidence: {}", rule.confidence);
            println!();
        }
    }

    Ok(())
}

/// Previews what tags would be inferred for a file
pub fn preview_tags(file: &Path, project_path: &Path) -> Result<()> {
    let sidecar_path = project_path.join(SIDECAR_FILENAME);

    if !sidecar_path.exists() {
        println!("No .code-indexer.yml found - no tag rules to apply");
        return Ok(());
    }

    let content = fs::read_to_string(&sidecar_path)?;
    let data = RootSidecarData::parse(&content)?;

    if data.tag_rules.is_empty() {
        println!("No tag rules defined");
        return Ok(());
    }

    // Get relative path from project root
    let file_path = file
        .strip_prefix(project_path)
        .unwrap_or(file)
        .to_string_lossy()
        .to_string();

    let matches = preview_tag_rules(&file_path, &data.tag_rules);

    if matches.is_empty() {
        println!("No rules match file: {}", file_path);
        return Ok(());
    }

    println!("Tags that would be inferred for '{}':\n", file_path);
    for m in &matches {
        println!("  Rule: {}", m.pattern);
        println!("    Tags: {}", m.tags.join(", "));
        println!("    Confidence: {}", m.confidence);
        println!();
    }

    // Show consolidated tags
    let inferred = apply_tag_rules(&file_path, &data.tag_rules);
    println!("Consolidated tags (with highest confidence):");
    for tag in &inferred {
        println!(
            "  {} (confidence: {}, from: {})",
            tag.tag, tag.confidence, tag.source_pattern
        );
    }

    Ok(())
}

/// Applies tag rules to all indexed files
pub fn apply_tags(project_path: &Path, db_path: &Path) -> Result<()> {
    let sidecar_path = project_path.join(SIDECAR_FILENAME);

    if !sidecar_path.exists() {
        println!("No .code-indexer.yml found - no tag rules to apply");
        return Ok(());
    }

    let content = fs::read_to_string(&sidecar_path)?;
    let data = RootSidecarData::parse(&content)?;

    if data.tag_rules.is_empty() {
        println!("No tag rules defined");
        return Ok(());
    }

    // Resolve effective db path
    let effective_db = if db_path == Path::new(".code-index.db") {
        project_path.join(".code-index.db")
    } else {
        db_path.to_path_buf()
    };

    let index = SqliteIndex::new(&effective_db)?;
    let tag_dict = index.get_tag_dictionary().unwrap_or_default();

    // Get all indexed files
    let stats = index.get_stats()?;
    println!(
        "Applying {} tag rules to {} files...",
        data.tag_rules.len(),
        stats.total_files
    );

    // Get file list from symbols (unique files)
    let files = index.get_indexed_files()?;
    let mut applied_count = 0;
    let mut warning_count = 0;

    for file_path in &files {
        // Get relative path from project root if absolute
        let relative_path = if Path::new(file_path).is_absolute() {
            Path::new(file_path)
                .strip_prefix(project_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file_path.clone())
        } else {
            file_path.clone()
        };

        let inferred = apply_tag_rules(&relative_path, &data.tag_rules);
        if !inferred.is_empty() {
            let result = resolve_inferred_tags(file_path, &inferred, &tag_dict);

            if !result.tags.is_empty() {
                if let Err(e) = index.add_file_tags(file_path, &result.tags) {
                    eprintln!("Warning: Failed to add tags to {}: {}", file_path, e);
                    warning_count += 1;
                } else {
                    applied_count += result.tags.len();
                }
            }

            for unknown in &result.unknown_tags {
                eprintln!("Warning: Unknown tag '{}' for file {}", unknown, file_path);
                warning_count += 1;
            }
        }
    }

    println!("Applied {} tags to files", applied_count);
    if warning_count > 0 {
        println!("Warnings: {}", warning_count);
    }

    Ok(())
}

/// Shows tag statistics from the index
pub fn show_tag_stats(db_path: &Path) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;

    match index.get_tag_stats() {
        Ok(stats) => {
            println!("Tag Statistics:\n");
            // stats is Vec<(category, tag_name, count)>
            println!(
                "Total tags: {}",
                stats.iter().map(|(_, _, count)| count).sum::<usize>()
            );
            println!();

            // Group by category
            let mut by_category: HashMap<String, Vec<(String, usize)>> = HashMap::new();
            for (category, tag_name, count) in stats {
                by_category
                    .entry(category)
                    .or_default()
                    .push((tag_name, count));
            }

            for (category, tags) in by_category {
                println!("{}:", category);
                for (tag, count) in tags {
                    println!("  {}: {} files", tag, count);
                }
                println!();
            }
        }
        Err(e) => {
            println!("Could not get tag stats: {}", e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::CodeIndex;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn index_directory_persists_content_hash_for_incremental_runs() {
        let temp_dir = TempDir::new().expect("temp dir");
        let source_path = temp_dir.path().join("lib.rs");
        let source = "pub fn alpha() {}\n";
        fs::write(&source_path, source).expect("write source");

        index_directory(
            temp_dir.path(),
            Path::new(".code-index.db"),
            false,
            IndexDurability::Safe,
        )
        .expect("index first run");

        let db_path = temp_dir.path().join(".code-index.db");
        let index = SqliteIndex::new(&db_path).expect("open db");
        let file_path = source_path.to_string_lossy().to_string();
        let expected_hash = SqliteIndex::compute_content_hash(source);

        assert_eq!(
            index.get_file_content_hash(&file_path).expect("read hash"),
            Some(expected_hash.clone())
        );
        assert!(!index
            .file_needs_reindex(&file_path, &expected_hash)
            .expect("hash compare"));

        index_directory(
            temp_dir.path(),
            Path::new(".code-index.db"),
            false,
            IndexDurability::Safe,
        )
        .expect("index second run");

        let index = SqliteIndex::new(&db_path).expect("reopen db");
        let defs = index.find_definition("alpha").expect("find definition");
        assert_eq!(defs.len(), 1);
        assert_eq!(
            index
                .get_file_content_hash(&file_path)
                .expect("read hash again"),
            Some(expected_hash)
        );
    }

    #[test]
    fn index_directory_removes_deleted_files_from_tracking() {
        let temp_dir = TempDir::new().expect("temp dir");
        let removed_path = temp_dir.path().join("removed.rs");
        let kept_path = temp_dir.path().join("kept.rs");
        fs::write(&removed_path, "pub fn removed() {}\n").expect("write removed");
        fs::write(&kept_path, "pub fn kept() {}\n").expect("write kept");

        index_directory(
            temp_dir.path(),
            Path::new(".code-index.db"),
            false,
            IndexDurability::Safe,
        )
        .expect("index first run");

        fs::remove_file(&removed_path).expect("remove source file");

        index_directory(
            temp_dir.path(),
            Path::new(".code-index.db"),
            false,
            IndexDurability::Safe,
        )
        .expect("index second run");

        let db_path = temp_dir.path().join(".code-index.db");
        let index = SqliteIndex::new(&db_path).expect("open db");
        let removed_str = removed_path.to_string_lossy().to_string();

        assert!(
            index
                .find_definition("removed")
                .expect("query removed")
                .is_empty(),
            "deleted file symbols must be removed from index"
        );
        assert_eq!(
            index
                .get_file_content_hash(&removed_str)
                .expect("removed hash lookup"),
            None
        );
        assert!(
            !index
                .get_tracked_files()
                .expect("tracked files")
                .contains(&removed_str),
            "deleted file must be removed from tracked files table"
        );
    }
}
