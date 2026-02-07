use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use clap::{Parser, Subcommand, ValueEnum};
use rayon::prelude::*;

use crate::error::Result;
use crate::index::sqlite::{IndexedFileRecord, SqliteIndex, TrackedFileMetadataUpdate};
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

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum IndexPowerProfile {
    Eco,
    Balanced,
    Max,
}

fn parse_positive_usize(value: &str) -> std::result::Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("Invalid usize value: {}", value))?;
    if parsed == 0 {
        return Err("Value must be >= 1".to_string());
    }
    Ok(parsed)
}

fn detect_available_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .max(1)
}

fn resolve_index_threads(
    profile: IndexPowerProfile,
    threads_override: Option<usize>,
    available_parallelism: usize,
) -> usize {
    if let Some(value) = threads_override {
        return value;
    }

    let available = available_parallelism.max(1);
    match profile {
        IndexPowerProfile::Eco => 1,
        IndexPowerProfile::Balanced => available.min(4),
        IndexPowerProfile::Max => available,
    }
}

fn profile_name(profile: IndexPowerProfile) -> &'static str {
    match profile {
        IndexPowerProfile::Eco => "eco",
        IndexPowerProfile::Balanced => "balanced",
        IndexPowerProfile::Max => "max",
    }
}

fn u64_to_i64_saturating(value: u64) -> i64 {
    if value > i64::MAX as u64 {
        i64::MAX
    } else {
        value as i64
    }
}

fn metadata_size_i64(metadata: &std::fs::Metadata) -> i64 {
    u64_to_i64_saturating(metadata.len())
}

fn metadata_mtime_ns_i64(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|mtime| mtime.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| {
            if duration.as_nanos() > i64::MAX as u128 {
                i64::MAX
            } else {
                duration.as_nanos() as i64
            }
        })
        .unwrap_or(0)
}

fn load_internal_agent_config(
    project_root: &Path,
) -> Option<crate::mcp::consolidated::InternalAgentConfig> {
    let sidecar_path = project_root.join(SIDECAR_FILENAME);
    let content = fs::read_to_string(sidecar_path).ok()?;
    let root = crate::indexer::RootSidecarData::parse(&content).ok()?;
    let agent = root.agent?;
    let provider = crate::indexer::normalize_agent_provider(agent.provider.as_deref())?;
    let (api_key, api_key_env) = crate::indexer::resolve_agent_api_key(&agent, &provider);

    Some(crate::mcp::consolidated::InternalAgentConfig {
        provider: Some(provider),
        model: agent.model,
        endpoint: agent.endpoint,
        api_key,
        api_key_env,
        mode: agent.mode.or_else(|| Some("planner".to_string())),
    })
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

        /// Indexing resource profile (safe default for laptops)
        #[arg(long, value_enum, default_value = "balanced")]
        profile: IndexPowerProfile,

        /// Limit rayon worker threads used for indexing
        #[arg(long, value_parser = parse_positive_usize)]
        threads: Option<usize>,

        /// Sleep between file parses to reduce sustained CPU temperature
        #[arg(long, default_value = "0")]
        throttle_ms: u64,
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

    /// Prepare AI-ready context bundle from a natural-language query
    /// (agent routing defaults are loaded from root .code-indexer.yml -> agent.*)
    PrepareContext {
        /// Query from external coding agent (Codex/Claude/etc.)
        query: String,

        /// Current file for locality-aware ranking
        #[arg(long)]
        file: Option<PathBuf>,

        /// Current cursor line in file
        #[arg(long)]
        line: Option<u32>,

        /// Current cursor column in file
        #[arg(long)]
        column: Option<u32>,

        /// Task hint (refactoring, debugging, understanding, implementing)
        #[arg(long)]
        task_hint: Option<String>,

        /// Maximum context items
        #[arg(long, default_value = "20")]
        max_items: usize,

        /// Approximate token budget for response
        #[arg(long)]
        approx_tokens: Option<usize>,

        /// Include code snippets in symbol cards
        #[arg(long)]
        include_snippets: bool,

        /// Snippet lines around symbol start
        #[arg(long, default_value = "3")]
        snippet_lines: usize,

        /// Output format: full, compact, minimal
        #[arg(long, default_value = "minimal")]
        format: String,

        /// Agent orchestration timeout in seconds
        #[arg(long, default_value = "60")]
        agent_timeout_sec: u64,

        /// Maximum number of agent orchestration steps
        #[arg(long, default_value = "6")]
        agent_max_steps: u32,

        /// Include detailed per-step collection trace (debug only)
        #[arg(long)]
        agent_include_trace: bool,

        /// Use already running MCP daemon over unix socket
        #[arg(long)]
        remote: Option<PathBuf>,
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
    profile: IndexPowerProfile,
    threads: Option<usize>,
    throttle_ms: u64,
) -> Result<()> {
    use code_indexer::indexer::ExtractionResult;

    #[derive(Clone)]
    struct IncrementalFileWorkItem {
        path: PathBuf,
        content: String,
        content_hash: String,
        last_size: i64,
        last_mtime_ns: i64,
    }

    #[derive(Clone)]
    struct ColdFileWorkItem {
        path: PathBuf,
    }

    // If db_path is the default value, place the database inside the indexed directory
    let effective_db = if db_path == Path::new(".code-index.db") {
        path.join(".code-index.db")
    } else {
        db_path.to_path_buf()
    };

    let walker = FileWalker::global();
    // CLI indexing writes in a single pipeline; one SQLite connection avoids lock churn.
    let index = SqliteIndex::with_pool_size(&effective_db, 1)?;
    let use_fast_bulk = durability == IndexDurability::Fast && !watch;
    let available_parallelism = detect_available_parallelism();
    let worker_threads = resolve_index_threads(profile, threads, available_parallelism);
    let index_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(worker_threads)
        .build()
        .map_err(|e| {
            crate::error::IndexerError::Index(format!(
                "Failed to create rayon thread pool with {} threads: {}",
                worker_threads, e
            ))
        })?;

    if durability == IndexDurability::Fast && watch {
        eprintln!("--durability fast is ignored in watch mode; using safe durability for updates.");
    }
    if threads.is_some() {
        println!(
            "Using {} indexing threads (manual override)",
            worker_threads
        );
    } else {
        println!(
            "Indexing profile: {} (threads={}, available={})",
            profile_name(profile),
            worker_threads,
            available_parallelism
        );
    }
    if throttle_ms > 0 {
        println!("Thermal throttle: {}ms sleep per file", throttle_ms);
    }

    let files = walker.walk(path)?;
    let tracked_states = index.get_tracked_file_states()?;
    let tracked_files: Vec<String> = tracked_states.keys().cloned().collect();
    let stale_files: Vec<String> = if tracked_files.is_empty() {
        Vec::new()
    } else {
        let current_files: HashSet<String> = files
            .iter()
            .map(|file| file.to_string_lossy().to_string())
            .collect();
        tracked_files
            .iter()
            .filter(|tracked| !current_files.contains(*tracked))
            .cloned()
            .collect()
    };
    if !stale_files.is_empty() {
        let stale_refs: Vec<&str> = stale_files.iter().map(|p| p.as_str()).collect();
        index.remove_files_batch(&stale_refs)?;
        println!("Removed {} stale files from index", stale_files.len());
    }

    // === Phase 1: Process sidecar files ===
    let sidecar_meta = process_sidecar_files(&files, path, &index)?;

    let is_cold_run = tracked_states.is_empty();

    let mut read_errors = 0usize;
    let mut unchanged_files = 0usize;
    let mut metadata_refresh_updates: Vec<TrackedFileMetadataUpdate> = Vec::new();
    let mut cold_files_to_index: Vec<ColdFileWorkItem> = Vec::new();
    let mut incremental_files_to_index: Vec<IncrementalFileWorkItem> = Vec::new();

    if is_cold_run {
        cold_files_to_index = files
            .iter()
            .cloned()
            .map(|path| ColdFileWorkItem { path })
            .collect();
    } else {
        enum PrecheckOutcome {
            Changed(IncrementalFileWorkItem),
            MetadataRefresh(TrackedFileMetadataUpdate),
            Unchanged,
            ReadError(PathBuf, std::io::Error),
        }

        let precheck_outcomes: Vec<PrecheckOutcome> = index_pool.install(|| {
            files
                .par_iter()
                .map(|file| {
                    let file_path = file.to_string_lossy().to_string();
                    let metadata = match fs::metadata(file) {
                        Ok(metadata) => metadata,
                        Err(err) => return PrecheckOutcome::ReadError(file.clone(), err),
                    };
                    let last_size = metadata_size_i64(&metadata);
                    let last_mtime_ns = metadata_mtime_ns_i64(&metadata);

                    if tracked_states
                        .get(&file_path)
                        .map(|state| {
                            state.last_size == Some(last_size)
                                && state.last_mtime_ns == Some(last_mtime_ns)
                        })
                        .unwrap_or(false)
                    {
                        return PrecheckOutcome::Unchanged;
                    }

                    match fs::read_to_string(file) {
                        Ok(content) => {
                            let content_hash = SqliteIndex::compute_content_hash(&content);
                            let needs_reindex = tracked_states
                                .get(&file_path)
                                .and_then(|state| state.content_hash.as_ref())
                                .map(|stored| stored != &content_hash)
                                .unwrap_or(true);
                            if needs_reindex {
                                PrecheckOutcome::Changed(IncrementalFileWorkItem {
                                    path: file.clone(),
                                    content,
                                    content_hash,
                                    last_size,
                                    last_mtime_ns,
                                })
                            } else {
                                PrecheckOutcome::MetadataRefresh(TrackedFileMetadataUpdate {
                                    path: file_path,
                                    last_size,
                                    last_mtime_ns,
                                })
                            }
                        }
                        Err(e) => PrecheckOutcome::ReadError(file.clone(), e),
                    }
                })
                .collect()
        });

        for outcome in precheck_outcomes {
            match outcome {
                PrecheckOutcome::Changed(item) => incremental_files_to_index.push(item),
                PrecheckOutcome::MetadataRefresh(item) => metadata_refresh_updates.push(item),
                PrecheckOutcome::Unchanged => unchanged_files += 1,
                PrecheckOutcome::ReadError(path, err) => {
                    read_errors += 1;
                    eprintln!("Error reading {}: {}", path.display(), err);
                }
            }
        }
    }

    let files_to_index_len = if is_cold_run {
        cold_files_to_index.len()
    } else {
        incremental_files_to_index.len()
    };

    if !is_cold_run {
        unchanged_files = files
            .len()
            .saturating_sub(incremental_files_to_index.len() + read_errors);
    }

    if unchanged_files > 0 {
        println!("Skipping {} unchanged files", unchanged_files);
    }

    // Set up progress tracking
    use crate::indexer::IndexingProgress;
    use indicatif::{ProgressBar, ProgressStyle};

    let progress = IndexingProgress::new();
    progress.start(files_to_index_len + read_errors);
    for _ in 0..read_errors {
        progress.inc_error();
    }

    let pb = ProgressBar::new(files_to_index_len as u64);
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
    let total_files = files_to_index_len;
    let files_done = AtomicUsize::new(0);
    let progress_batch_size = 32usize;
    let per_file_delay = std::time::Duration::from_millis(throttle_ms);
    const CHUNK_FILES: usize = 256;
    const CHUNK_SYMBOLS: usize = 100_000;

    let mut total_symbols = 0usize;
    let mut indexed_files = 0usize;
    let mut pending_symbols = 0usize;
    let mut pending_file_records: Vec<IndexedFileRecord> = Vec::new();
    let mut pending_extraction_results: Vec<ExtractionResult> = Vec::new();

    fn flush_pending_chunk(
        index: &SqliteIndex,
        sidecar_meta: &HashSet<String>,
        pending_file_records: &mut Vec<IndexedFileRecord>,
        pending_extraction_results: &mut Vec<ExtractionResult>,
        use_fast_bulk: bool,
        is_cold_run: bool,
    ) -> Result<(usize, usize)> {
        if pending_extraction_results.is_empty() {
            return Ok((0, 0));
        }

        update_exported_hashes(pending_extraction_results, sidecar_meta, index)?;
        if !is_cold_run {
            let changed_paths: Vec<&str> = pending_file_records
                .iter()
                .map(|record| record.path.as_str())
                .collect();
            if !changed_paths.is_empty() {
                index.remove_files_batch(&changed_paths)?;
            }
        }

        let extraction_batch = std::mem::take(pending_extraction_results);
        let records_batch = std::mem::take(pending_file_records);
        let file_count = records_batch.len();
        let symbol_count = index.add_extraction_results_batch_with_mode(
            extraction_batch,
            use_fast_bulk,
            is_cold_run,
        )?;
        index.upsert_file_records_batch(&records_batch)?;
        Ok((file_count, symbol_count))
    }

    if is_cold_run {
        let mut remaining = cold_files_to_index;
        while !remaining.is_empty() {
            let take = CHUNK_FILES.min(remaining.len());
            let chunk: Vec<ColdFileWorkItem> = remaining.drain(..take).collect();
            let collect_results = || {
                chunk
                    .into_par_iter()
                    .map_init(
                        || (CodeParser::global(), SymbolExtractor::new()),
                        |(parser, extractor), file| {
                            if throttle_ms > 0 {
                                std::thread::sleep(per_file_delay);
                            }

                            match parser.parse_file(&file.path) {
                                Ok(parsed) => match extractor.extract_all(&parsed, &file.path) {
                                    Ok(result) => {
                                        let symbol_count = result.symbols.len();
                                        let metadata = fs::metadata(&file.path).ok();
                                        let file_record = IndexedFileRecord {
                                            path: file.path.to_string_lossy().to_string(),
                                            language: parsed.language.clone(),
                                            symbol_count,
                                            content_hash: SqliteIndex::compute_content_hash(
                                                &parsed.source,
                                            ),
                                            last_size: metadata
                                                .as_ref()
                                                .map(metadata_size_i64)
                                                .unwrap_or(0),
                                            last_mtime_ns: metadata
                                                .as_ref()
                                                .map(metadata_mtime_ns_i64)
                                                .unwrap_or(0),
                                        };
                                        progress_ref.inc(symbol_count);
                                        let completed =
                                            files_done.fetch_add(1, Ordering::Relaxed) + 1;
                                        if completed % progress_batch_size == 0
                                            || completed == total_files
                                        {
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
                                        let completed =
                                            files_done.fetch_add(1, Ordering::Relaxed) + 1;
                                        if completed % progress_batch_size == 0
                                            || completed == total_files
                                        {
                                            pb_ref.set_position(completed as u64);
                                        }
                                        None
                                    }
                                },
                                Err(e) => {
                                    eprintln!("Error parsing {}: {}", file.path.display(), e);
                                    progress_ref.inc_error();
                                    let completed = files_done.fetch_add(1, Ordering::Relaxed) + 1;
                                    if completed % progress_batch_size == 0
                                        || completed == total_files
                                    {
                                        pb_ref.set_position(completed as u64);
                                    }
                                    None
                                }
                            }
                        },
                    )
                    .filter_map(|r| r)
                    .collect::<Vec<(IndexedFileRecord, ExtractionResult)>>()
            };
            let chunk_results = index_pool.install(collect_results);
            for (file_record, extraction_result) in chunk_results {
                pending_symbols += extraction_result.symbols.len();
                pending_file_records.push(file_record);
                pending_extraction_results.push(extraction_result);
            }
            if pending_file_records.len() >= CHUNK_FILES || pending_symbols >= CHUNK_SYMBOLS {
                let (chunk_files, chunk_symbols) = flush_pending_chunk(
                    &index,
                    &sidecar_meta,
                    &mut pending_file_records,
                    &mut pending_extraction_results,
                    use_fast_bulk,
                    is_cold_run,
                )?;
                indexed_files += chunk_files;
                total_symbols += chunk_symbols;
                pending_symbols = 0;
            }
        }
    } else {
        let parse_cache = ParseCache::new();
        let parse_cache_ref = &parse_cache;
        let mut remaining = incremental_files_to_index;
        while !remaining.is_empty() {
            let take = CHUNK_FILES.min(remaining.len());
            let chunk: Vec<IncrementalFileWorkItem> = remaining.drain(..take).collect();
            let collect_results = || {
                chunk
                    .into_par_iter()
                    .map_init(
                        || (CodeParser::global(), SymbolExtractor::new()),
                        |(parser, extractor), item| {
                            if throttle_ms > 0 {
                                std::thread::sleep(per_file_delay);
                            }

                            let IncrementalFileWorkItem {
                                path,
                                content,
                                content_hash,
                                last_size,
                                last_mtime_ns,
                            } = item;

                            match parse_cache_ref.parse_source_cached_owned(&path, content, parser)
                            {
                                Ok(parsed) => match extractor.extract_all(&parsed, &path) {
                                    Ok(result) => {
                                        let symbol_count = result.symbols.len();
                                        let file_record = IndexedFileRecord {
                                            path: path.to_string_lossy().to_string(),
                                            language: parsed.language.clone(),
                                            symbol_count,
                                            content_hash,
                                            last_size,
                                            last_mtime_ns,
                                        };
                                        progress_ref.inc(symbol_count);
                                        let completed =
                                            files_done.fetch_add(1, Ordering::Relaxed) + 1;
                                        if completed % progress_batch_size == 0
                                            || completed == total_files
                                        {
                                            pb_ref.set_position(completed as u64);
                                        }
                                        Some((file_record, result))
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "Error extracting symbols from {}: {}",
                                            path.display(),
                                            e
                                        );
                                        progress_ref.inc_error();
                                        let completed =
                                            files_done.fetch_add(1, Ordering::Relaxed) + 1;
                                        if completed % progress_batch_size == 0
                                            || completed == total_files
                                        {
                                            pb_ref.set_position(completed as u64);
                                        }
                                        None
                                    }
                                },
                                Err(e) => {
                                    eprintln!("Error parsing {}: {}", path.display(), e);
                                    parse_cache_ref.invalidate(&path);
                                    progress_ref.inc_error();
                                    let completed = files_done.fetch_add(1, Ordering::Relaxed) + 1;
                                    if completed % progress_batch_size == 0
                                        || completed == total_files
                                    {
                                        pb_ref.set_position(completed as u64);
                                    }
                                    None
                                }
                            }
                        },
                    )
                    .filter_map(|r| r)
                    .collect::<Vec<(IndexedFileRecord, ExtractionResult)>>()
            };
            let chunk_results = index_pool.install(collect_results);
            for (file_record, extraction_result) in chunk_results {
                pending_symbols += extraction_result.symbols.len();
                pending_file_records.push(file_record);
                pending_extraction_results.push(extraction_result);
            }
            if pending_file_records.len() >= CHUNK_FILES || pending_symbols >= CHUNK_SYMBOLS {
                let (chunk_files, chunk_symbols) = flush_pending_chunk(
                    &index,
                    &sidecar_meta,
                    &mut pending_file_records,
                    &mut pending_extraction_results,
                    use_fast_bulk,
                    is_cold_run,
                )?;
                indexed_files += chunk_files;
                total_symbols += chunk_symbols;
                pending_symbols = 0;
            }
        }
    }

    let (chunk_files, chunk_symbols) = flush_pending_chunk(
        &index,
        &sidecar_meta,
        &mut pending_file_records,
        &mut pending_extraction_results,
        use_fast_bulk,
        is_cold_run,
    )?;
    indexed_files += chunk_files;
    total_symbols += chunk_symbols;
    if !metadata_refresh_updates.is_empty() {
        index.update_file_tracking_metadata_batch(&metadata_refresh_updates)?;
    }

    pb.finish_with_message(format!(
        "{} symbols from {} changed files",
        total_symbols, indexed_files
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
        let walker = FileWalker::global();
        let parser = CodeParser::global();
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
                                        let metadata = fs::metadata(&file_path).ok();
                                        let file_record = IndexedFileRecord {
                                            path: file_path.to_string_lossy().to_string(),
                                            language: parsed.language.clone(),
                                            symbol_count: count,
                                            content_hash,
                                            last_size: metadata
                                                .as_ref()
                                                .map(metadata_size_i64)
                                                .unwrap_or(0),
                                            last_mtime_ns: metadata
                                                .as_ref()
                                                .map(metadata_mtime_ns_i64)
                                                .unwrap_or(0),
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

#[allow(clippy::too_many_arguments)]
pub async fn prepare_context(
    db_path: &Path,
    query: &str,
    file: Option<PathBuf>,
    line: Option<u32>,
    column: Option<u32>,
    task_hint: Option<String>,
    max_items: usize,
    approx_tokens: Option<usize>,
    include_snippets: bool,
    snippet_lines: usize,
    format: &str,
    agent_timeout_sec: u64,
    agent_max_steps: u32,
    agent_include_trace: bool,
    remote: Option<&Path>,
) -> Result<()> {
    use crate::mcp::consolidated::PrepareContextParams;

    let agent = load_internal_agent_config(Path::new("."));

    let params = PrepareContextParams {
        query: query.to_string(),
        file: file.map(|p| p.to_string_lossy().to_string()),
        line,
        column,
        task_hint,
        max_items: Some(max_items),
        approx_tokens,
        include_snippets: Some(include_snippets),
        snippet_lines: Some(snippet_lines),
        format: Some(format.to_string()),
        envelope: Some(true),
        agent,
        agent_timeout_ms: Some(agent_timeout_sec.saturating_mul(1000)),
        agent_max_steps: Some(agent_max_steps),
        include_trace: Some(agent_include_trace),
    };

    if let Some(socket) = remote {
        let args_value = serde_json::to_value(&params).map_err(|e| {
            crate::error::IndexerError::Index(format!(
                "Failed to serialize prepare_context params: {}",
                e
            ))
        })?;
        let args = match args_value {
            serde_json::Value::Object(map) => map,
            _ => {
                return Err(crate::error::IndexerError::Index(
                    "prepare_context params must serialize to object".to_string(),
                ))
            }
        };

        let output = call_remote_tool(socket, "prepare_context", args).await?;
        if !output.is_empty() {
            println!("{}", output);
        }
        return Ok(());
    }

    let index = Arc::new(open_query_index(db_path)?);
    let server = crate::mcp::McpServer::new(index);
    let envelope = server.prepare_context_with_agent(params).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&envelope).unwrap_or_default()
    );

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
/// Returns a set of file paths that had sidecar metadata.
fn process_sidecar_files(
    files: &[PathBuf],
    root: &Path,
    index: &SqliteIndex,
) -> Result<HashSet<String>> {
    let mut sidecar_dirs: HashSet<PathBuf> = HashSet::new();
    for file in files {
        if let Some(parent) = file.parent() {
            let dir = parent.to_path_buf();
            if sidecar_dirs.contains(&dir) {
                continue;
            }
            if dir.join(SIDECAR_FILENAME).exists() {
                sidecar_dirs.insert(dir);
            }
        }
    }

    if sidecar_dirs.is_empty() {
        return Ok(HashSet::new());
    }

    let mut files_by_dir: HashMap<PathBuf, Vec<&PathBuf>> = HashMap::new();
    for file in files {
        if let Some(parent) = file.parent() {
            if sidecar_dirs.contains(parent) {
                files_by_dir
                    .entry(parent.to_path_buf())
                    .or_default()
                    .push(file);
            }
        }
    }

    // Get tag dictionary for resolution
    let tag_dict = index.get_tag_dictionary().unwrap_or_default();

    let mut files_with_meta: HashSet<String> = HashSet::new();
    let mut sidecar_count = 0;

    // Process each directory's sidecar
    for (dir, dir_files) in files_by_dir {
        let sidecar_path = dir.join(SIDECAR_FILENAME);

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
        let mut meta_batch = Vec::new();
        let mut tag_batch: Vec<(String, Vec<crate::index::FileTag>)> = Vec::new();

        // Process files in this directory that are listed in the sidecar
        for file in dir_files {
            let file_path_str = file.to_string_lossy().to_string();

            // Make relative path for sidecar lookup
            let relative_path = file
                .strip_prefix(root)
                .unwrap_or(file)
                .to_string_lossy()
                .to_string();

            // Extract file metadata from sidecar
            if let Some(meta) = extract_file_meta(&relative_path, &sidecar_data, &dir_str) {
                files_with_meta.insert(file_path_str.clone());
                meta_batch.push(meta);
            }

            // Extract and save file tags
            let tag_strings = extract_file_tags(&relative_path, &sidecar_data);
            if !tag_strings.is_empty() {
                let file_tags = resolve_tags(&file_path_str, &tag_strings, &tag_dict);
                if !file_tags.is_empty() {
                    tag_batch.push((file_path_str, file_tags));
                }
            }
        }

        if !meta_batch.is_empty() {
            if let Err(e) = index.upsert_file_meta_batch(&meta_batch) {
                eprintln!("Warning: Could not save sidecar metadata batch: {}", e);
            }
        }
        if !tag_batch.is_empty() {
            if let Err(e) = index.add_file_tags_batch(&tag_batch) {
                eprintln!("Warning: Could not save sidecar tags batch: {}", e);
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
    let mut meta_batch = Vec::new();
    let mut tag_batch: Vec<(String, Vec<crate::index::FileTag>)> = Vec::new();

    for file in files {
        let file_path_str = file.to_string_lossy().to_string();
        let relative_path = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .to_string_lossy()
            .to_string();

        // Update file metadata
        if let Some(meta) = extract_file_meta(&relative_path, &sidecar_data, &dir_str) {
            updated_count += 1;
            meta_batch.push(meta);
        }

        // Update file tags
        let tag_strings = extract_file_tags(&relative_path, &sidecar_data);
        if !tag_strings.is_empty() {
            let file_tags = resolve_tags(&file_path_str, &tag_strings, &tag_dict);
            if !file_tags.is_empty() {
                tag_batch.push((file_path_str, file_tags));
            }
        }
    }

    if !meta_batch.is_empty() {
        let _ = index.upsert_file_meta_batch(&meta_batch);
    }
    if !tag_batch.is_empty() {
        let _ = index.add_file_tags_batch(&tag_batch);
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
    files_with_meta: &HashSet<String>,
    index: &SqliteIndex,
) -> Result<()> {
    let mut exported_by_file: HashMap<String, String> = HashMap::new();
    for result in results {
        // Get file_path from first symbol's location
        let file_path = match result.symbols.first() {
            Some(symbol) => symbol.location.file_path.clone(),
            None => continue, // Skip files with no symbols
        };

        let has_sidecar_meta = files_with_meta.contains(&file_path);
        let has_exported_symbols = result.symbols.iter().any(|symbol| {
            matches!(
                symbol.visibility,
                Some(crate::index::Visibility::Public) | Some(crate::index::Visibility::Internal)
            )
        });

        // Avoid storing inferred metadata for files that have no exported API surface.
        if !has_sidecar_meta && !has_exported_symbols {
            continue;
        }

        // Compute exported hash from public symbols
        let exported_hash = compute_exported_hash(&result.symbols);
        exported_by_file.insert(file_path, exported_hash);
    }

    if exported_by_file.is_empty() {
        return Ok(());
    }

    let file_paths: Vec<String> = exported_by_file.keys().cloned().collect();
    let mut existing = index.get_file_meta_many(&file_paths)?;
    let mut updates = Vec::with_capacity(file_paths.len());

    for file_path in file_paths {
        let has_sidecar_meta = files_with_meta.contains(&file_path);
        let target_hash = exported_by_file.get(&file_path).cloned();

        if let Some(mut meta) = existing.remove(&file_path) {
            let mut changed = false;
            if meta.exported_hash != target_hash {
                meta.exported_hash = target_hash.clone();
                changed = true;
            }
            if !has_sidecar_meta {
                if meta.source != MetaSource::Inferred {
                    meta.source = MetaSource::Inferred;
                    changed = true;
                }
                if meta.confidence != 0.5 {
                    meta.confidence = 0.5;
                    changed = true;
                }
            }
            if changed {
                updates.push(meta);
            }
            continue;
        }

        let mut meta = FileMeta::new(&file_path);
        meta.exported_hash = target_hash;
        if has_sidecar_meta {
            meta.source = MetaSource::Sidecar;
            meta.confidence = 1.0;
        } else {
            meta.source = MetaSource::Inferred;
            meta.confidence = 0.5;
        }
        updates.push(meta);
    }

    if updates.is_empty() {
        return Ok(());
    }

    index.upsert_file_meta_batch(&updates)?;
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
            IndexPowerProfile::Balanced,
            None,
            0,
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
            IndexPowerProfile::Balanced,
            None,
            0,
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
            IndexPowerProfile::Balanced,
            None,
            0,
        )
        .expect("index first run");

        fs::remove_file(&removed_path).expect("remove source file");

        index_directory(
            temp_dir.path(),
            Path::new(".code-index.db"),
            false,
            IndexDurability::Safe,
            IndexPowerProfile::Balanced,
            None,
            0,
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

    #[test]
    fn index_directory_skips_inferred_meta_for_files_without_exported_symbols() {
        let temp_dir = TempDir::new().expect("temp dir");
        let source_path = temp_dir.path().join("private_only.rs");
        fs::write(&source_path, "fn helper() {}\n").expect("write source");

        index_directory(
            temp_dir.path(),
            Path::new(".code-index.db"),
            false,
            IndexDurability::Safe,
            IndexPowerProfile::Balanced,
            None,
            0,
        )
        .expect("index run");

        let db_path = temp_dir.path().join(".code-index.db");
        let index = SqliteIndex::new(&db_path).expect("open db");
        let file_path = source_path.to_string_lossy().to_string();

        assert!(
            index.get_file_meta(&file_path).expect("read file_meta").is_none(),
            "file without sidecar and without exported API should not create inferred file_meta row"
        );
    }

    #[test]
    fn index_directory_keeps_sidecar_meta_for_private_only_files() {
        let temp_dir = TempDir::new().expect("temp dir");
        let source_path = temp_dir.path().join("private_only.rs");
        fs::write(&source_path, "fn helper() {}\n").expect("write source");
        fs::write(
            temp_dir.path().join(SIDECAR_FILENAME),
            "files:\n  private_only.rs:\n    doc1: \"Private helper\"\n",
        )
        .expect("write sidecar");

        index_directory(
            temp_dir.path(),
            Path::new(".code-index.db"),
            false,
            IndexDurability::Safe,
            IndexPowerProfile::Balanced,
            None,
            0,
        )
        .expect("index run");

        let db_path = temp_dir.path().join(".code-index.db");
        let index = SqliteIndex::new(&db_path).expect("open db");
        let file_path = source_path.to_string_lossy().to_string();
        let meta = index
            .get_file_meta(&file_path)
            .expect("read file_meta")
            .expect("sidecar meta should exist");

        assert_eq!(meta.source, MetaSource::Sidecar);
        assert!(
            meta.exported_hash.is_some(),
            "sidecar-managed file should preserve exported hash even without public symbols"
        );
    }

    #[test]
    fn cli_index_threads_flag_parses() {
        let cli = Cli::try_parse_from([
            "code-indexer",
            "index",
            ".",
            "--profile",
            "eco",
            "--threads",
            "3",
            "--throttle-ms",
            "10",
            "--durability",
            "safe",
        ])
        .expect("cli parse must succeed");

        match cli.command {
            Commands::Index {
                profile,
                threads,
                throttle_ms,
                ..
            } => {
                assert_eq!(profile, IndexPowerProfile::Eco);
                assert_eq!(threads, Some(3));
                assert_eq!(throttle_ms, 10);
            }
            _ => panic!("expected index command"),
        }
    }

    #[test]
    fn cli_index_threads_rejects_zero() {
        let parsed = Cli::try_parse_from(["code-indexer", "index", ".", "--threads", "0"]);
        assert!(parsed.is_err());
    }

    #[test]
    fn cli_index_profile_defaults_to_balanced() {
        let cli = Cli::try_parse_from(["code-indexer", "index", "."]).expect("cli parse");

        match cli.command {
            Commands::Index {
                profile,
                threads,
                throttle_ms,
                ..
            } => {
                assert_eq!(profile, IndexPowerProfile::Balanced);
                assert_eq!(threads, None);
                assert_eq!(throttle_ms, 0);
            }
            _ => panic!("expected index command"),
        }
    }

    #[test]
    fn resolve_index_threads_balanced_caps_to_four() {
        assert_eq!(
            resolve_index_threads(IndexPowerProfile::Balanced, None, 10),
            4
        );
        assert_eq!(
            resolve_index_threads(IndexPowerProfile::Balanced, None, 2),
            2
        );
    }

    #[test]
    fn resolve_index_threads_override_wins() {
        assert_eq!(
            resolve_index_threads(IndexPowerProfile::Eco, Some(3), 16),
            3
        );
        assert_eq!(resolve_index_threads(IndexPowerProfile::Max, Some(2), 1), 2);
    }

    #[test]
    fn cli_prepare_context_parses() {
        let cli = Cli::try_parse_from([
            "code-indexer",
            "prepare-context",
            "find auth flow",
            "--file",
            "src/auth/mod.rs",
            "--line",
            "10",
            "--column",
            "3",
            "--task-hint",
            "debugging",
            "--max-items",
            "15",
            "--approx-tokens",
            "4096",
            "--include-snippets",
            "--snippet-lines",
            "5",
            "--format",
            "minimal",
        ])
        .expect("cli parse");

        match cli.command {
            Commands::PrepareContext {
                query,
                file,
                line,
                column,
                task_hint,
                max_items,
                approx_tokens,
                include_snippets,
                snippet_lines,
                format,
                agent_timeout_sec,
                agent_max_steps,
                agent_include_trace,
                remote,
            } => {
                assert_eq!(query, "find auth flow");
                assert_eq!(
                    file.map(|p| p.to_string_lossy().to_string()),
                    Some("src/auth/mod.rs".to_string())
                );
                assert_eq!(line, Some(10));
                assert_eq!(column, Some(3));
                assert_eq!(task_hint.as_deref(), Some("debugging"));
                assert_eq!(max_items, 15);
                assert_eq!(approx_tokens, Some(4096));
                assert!(include_snippets);
                assert_eq!(snippet_lines, 5);
                assert_eq!(format, "minimal");
                assert_eq!(agent_timeout_sec, 60);
                assert_eq!(agent_max_steps, 6);
                assert!(!agent_include_trace);
                assert!(remote.is_none());
            }
            _ => panic!("expected prepare-context command"),
        }
    }

    #[test]
    fn cli_prepare_context_agent_flags_parse() {
        let cli = Cli::try_parse_from([
            "code-indexer",
            "prepare-context",
            "trace auth",
            "--agent-timeout-sec",
            "90",
            "--agent-max-steps",
            "9",
            "--agent-include-trace",
        ])
        .expect("cli parse");

        match cli.command {
            Commands::PrepareContext {
                agent_timeout_sec,
                agent_max_steps,
                agent_include_trace,
                ..
            } => {
                assert_eq!(agent_timeout_sec, 90);
                assert_eq!(agent_max_steps, 9);
                assert!(agent_include_trace);
            }
            _ => panic!("expected prepare-context command"),
        }
    }

    #[test]
    fn load_internal_agent_config_from_root_sidecar() {
        let temp_dir = TempDir::new().expect("temp dir");
        let sidecar_path = temp_dir.path().join(SIDECAR_FILENAME);
        let env_name = "CODE_INDEXER_TEST_OPENROUTER_TOKEN";
        std::env::set_var(env_name, "test-token-from-env");
        fs::write(
            &sidecar_path,
            r#"
agent:
  provider: openrouter
  model: openrouter/auto
  endpoint: https://openrouter.ai/api/v1
  api_key_env: CODE_INDEXER_TEST_OPENROUTER_TOKEN
"#,
        )
        .expect("write sidecar");

        let agent = load_internal_agent_config(temp_dir.path()).expect("agent config");
        assert_eq!(agent.provider.as_deref(), Some("openrouter"));
        assert_eq!(agent.model.as_deref(), Some("openrouter/auto"));
        assert_eq!(
            agent.endpoint.as_deref(),
            Some("https://openrouter.ai/api/v1")
        );
        assert_eq!(agent.api_key_env.as_deref(), Some(env_name));
        assert_eq!(agent.api_key.as_deref(), Some("test-token-from-env"));
        assert_eq!(agent.mode.as_deref(), Some("planner"));
        std::env::remove_var(env_name);
    }

    #[test]
    fn load_internal_agent_config_without_provider_returns_none() {
        let temp_dir = TempDir::new().expect("temp dir");
        let sidecar_path = temp_dir.path().join(SIDECAR_FILENAME);
        fs::write(
            &sidecar_path,
            r#"
agent:
  model: openrouter/auto
"#,
        )
        .expect("write sidecar");

        assert!(load_internal_agent_config(temp_dir.path()).is_none());
    }
}
