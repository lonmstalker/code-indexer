use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Parser, Subcommand};
use rayon::prelude::*;

use code_indexer::dependencies::{DependencyRegistry, ProjectInfo};
use code_indexer::git::GitAnalyzer;
use crate::error::Result;
use crate::index::sqlite::SqliteIndex;
use crate::index::{CodeIndex, SearchOptions, OutputFormat, CompactSymbol};
use crate::indexer::watcher::FileEvent;
use crate::indexer::{FileWalker, FileWatcher, Parser as CodeParser, SymbolExtractor};
use crate::languages::LanguageRegistry;

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
    },

    /// Start MCP server
    Serve,

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
    },

    /// Get file imports
    Imports {
        /// File path
        file: PathBuf,

        /// Resolve imports to their definitions
        #[arg(long)]
        resolve: bool,
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
    Stats,

    /// Clear the index
    Clear,

    /// Work with project dependencies
    Deps {
        #[command(subcommand)]
        command: DepsCommands,
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
    Definition {
        name: String,
    },

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

pub fn index_directory(path: &PathBuf, db_path: &PathBuf, watch: bool) -> Result<()> {
    use code_indexer::indexer::ExtractionResult;

    // If db_path is the default value, place the database inside the indexed directory
    let effective_db = if db_path == Path::new(".code-index.db") {
        path.join(".code-index.db")
    } else {
        db_path.clone()
    };

    let registry = LanguageRegistry::new();
    let walker = FileWalker::new(registry);
    let index = SqliteIndex::new(&effective_db)?;

    let files = walker.walk(path)?;
    println!("Found {} files to index", files.len());

    // Parallel parsing and extraction using rayon
    let results: Vec<ExtractionResult> = files
        .par_iter()
        .filter_map(|file| {
            // Each thread gets its own parser and extractor
            let registry = LanguageRegistry::new();
            let parser = CodeParser::new(registry);
            let extractor = SymbolExtractor::new();

            match parser.parse_file(file) {
                Ok(parsed) => match extractor.extract_all(&parsed, file) {
                    Ok(result) => Some(result),
                    Err(e) => {
                        eprintln!("Error extracting symbols from {}: {}", file.display(), e);
                        None
                    }
                },
                Err(e) => {
                    eprintln!("Error parsing {}: {}", file.display(), e);
                    None
                }
            }
        })
        .collect();

    // Batch insert all results
    let total_symbols = index.add_extraction_results_batch(results)?;

    println!(
        "Indexed {} symbols from {} files",
        total_symbols,
        files.len()
    );

    if watch {
        println!("Watching for changes...");
        let watcher = FileWatcher::new(path)?;
        let registry = LanguageRegistry::new();
        let walker = FileWalker::new(registry);
        let registry = LanguageRegistry::new();
        let parser = CodeParser::new(registry);
        let extractor = SymbolExtractor::new();

        loop {
            if let Some(events) = watcher.recv() {
                for event in events {
                    match event {
                        FileEvent::Modified(file_path) | FileEvent::Created(file_path) => {
                            if walker.is_supported(&file_path) {
                                index.remove_file(&file_path.to_string_lossy())?;
                                if let Ok(parsed) = parser.parse_file(&file_path) {
                                    if let Ok(result) = extractor.extract_all(&parsed, &file_path) {
                                        let count = result.symbols.len();
                                        index.add_extraction_results_batch(vec![result])?;
                                        println!("Updated {}: {} symbols", file_path.display(), count);
                                    }
                                }
                            }
                        }
                        FileEvent::Deleted(file_path) => {
                            index.remove_file(&file_path.to_string_lossy())?;
                            println!("Removed {}", file_path.display());
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub async fn run_mcp_server(db_path: &PathBuf) -> Result<()> {
    use crate::mcp::McpServer;
    use rmcp::ServiceExt;

    let index = Arc::new(SqliteIndex::new(db_path)?);
    let server = McpServer::new(index);

    let transport = (tokio::io::stdin(), tokio::io::stdout());
    server.serve(transport).await.map_err(|e| {
        crate::error::IndexerError::Mcp(e.to_string())
    })?;

    Ok(())
}

pub fn search_symbols(
    db_path: &PathBuf,
    query: &str,
    limit: usize,
    format: &str,
    fuzzy: bool,
    fuzzy_threshold: f64,
) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
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

pub fn find_definition(db_path: &PathBuf, name: &str) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
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
    db_path: &PathBuf,
    limit: usize,
    language: Option<String>,
    file: Option<String>,
    pattern: Option<String>,
    format: &str,
) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
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
    db_path: &PathBuf,
    limit: usize,
    language: Option<String>,
    file: Option<String>,
    pattern: Option<String>,
    format: &str,
) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
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

pub fn show_stats(db_path: &PathBuf) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
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

pub fn clear_index(db_path: &PathBuf) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
    index.clear()?;
    println!("Index cleared");
    Ok(())
}

// === New consolidated commands ===

/// Unified symbols command (replaces search, list_functions, list_types)
pub fn symbols(
    db_path: &PathBuf,
    query: Option<String>,
    kind: &str,
    limit: usize,
    language: Option<String>,
    file: Option<String>,
    pattern: Option<String>,
    format: &str,
    fuzzy: bool,
    fuzzy_threshold: f64,
) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
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
pub fn find_references(
    db_path: &PathBuf,
    name: &str,
    include_callers: bool,
    depth: u32,
    file: Option<String>,
    limit: usize,
) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;

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
                        println!(
                            "  {} at {}:{}",
                            c.symbol_name, c.file_path, c.line
                        );
                    }
                }
            }
            Err(e) => println!("  Error finding callers: {}", e),
        }
    }

    Ok(())
}

/// Call graph analysis command
pub fn analyze_call_graph(
    db_path: &PathBuf,
    function: &str,
    direction: &str,
    depth: u32,
    _include_possible: bool,
) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;

    println!("Call graph for '{}' (direction: {}, depth: {}):", function, direction, depth);

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
pub fn get_outline(
    db_path: &PathBuf,
    file: &PathBuf,
    start_line: Option<u32>,
    end_line: Option<u32>,
    include_scopes: bool,
) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
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
            let indent = if symbol.parent.is_some() { "    " } else { "  " };
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
pub fn get_imports(db_path: &PathBuf, file: &PathBuf, resolve: bool) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
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
    path: &PathBuf,
    db_path: &PathBuf,
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
        println!(
            "Project: {} ({})",
            project.name,
            project.ecosystem.as_str()
        );
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
    path: &PathBuf,
    db_path: &PathBuf,
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

    for dep in deps_to_index {
        let source_path = dep.source_path.as_ref().unwrap();
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

        index.mark_dependency_indexed(dep_id)?;
        println!(" {} symbols from {} files", total_symbols, files.len());
    }

    Ok(())
}

/// Finds a symbol in indexed dependencies.
pub fn find_in_dependencies(
    db_path: &PathBuf,
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
    db_path: &PathBuf,
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
pub fn show_dependency_info(path: &PathBuf, db_path: &PathBuf, name: &str) -> Result<()> {
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
            println!(
                "Indexed: {}",
                if db_dep.is_indexed { "yes" } else { "no" }
            );
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

/// Gets symbols from changed files (git diff)
pub fn get_changed_symbols(
    db_path: &PathBuf,
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

    let changed_symbols = git.find_changed_symbols(&index, base, include_staged, include_unstaged)?;

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
