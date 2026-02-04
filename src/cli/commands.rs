use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Parser, Subcommand};
use rayon::prelude::*;

use code_indexer::dependencies::{DependencyRegistry, ProjectInfo};
use crate::error::Result;
use crate::index::sqlite::SqliteIndex;
use crate::index::{CodeIndex, SearchOptions};
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
    },

    /// Start MCP server
    Serve,

    /// Query the index
    Query {
        #[command(subcommand)]
        query: QueryCommands,
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
}

#[derive(Subcommand)]
pub enum QueryCommands {
    /// Search for symbols
    Search {
        /// Search query
        query: String,

        /// Maximum number of results
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Find symbol definition
    Definition {
        /// Symbol name
        name: String,
    },

    /// List functions
    Functions {
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
    },

    /// List types
    Types {
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

pub fn search_symbols(db_path: &PathBuf, query: &str, limit: usize) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
    let options = SearchOptions {
        limit: Some(limit),
        ..Default::default()
    };

    let results = index.search(query, &options)?;

    if results.is_empty() {
        println!("No symbols found for query: {}", query);
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
) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
    let options = SearchOptions {
        limit: Some(limit),
        language_filter: language.map(|l| vec![l]),
        file_filter: file,
        name_filter: pattern,
        ..Default::default()
    };

    let symbols = index.list_functions(&options)?;

    if symbols.is_empty() {
        println!("No functions found");
        return Ok(());
    }

    for symbol in symbols {
        let kind = symbol.kind.as_str();
        let location = format!(
            "{}:{}",
            symbol.location.file_path, symbol.location.start_line
        );
        println!("{} ({}) - {}", symbol.name, kind, location);
    }

    Ok(())
}

pub fn list_types(
    db_path: &PathBuf,
    limit: usize,
    language: Option<String>,
    file: Option<String>,
    pattern: Option<String>,
) -> Result<()> {
    let index = SqliteIndex::new(db_path)?;
    let options = SearchOptions {
        limit: Some(limit),
        language_filter: language.map(|l| vec![l]),
        file_filter: file,
        name_filter: pattern,
        ..Default::default()
    };

    let symbols = index.list_types(&options)?;

    if symbols.is_empty() {
        println!("No types found");
        return Ok(());
    }

    for symbol in symbols {
        let kind = symbol.kind.as_str();
        let location = format!(
            "{}:{}",
            symbol.location.file_path, symbol.location.start_line
        );
        println!("{} ({}) - {}", symbol.name, kind, location);
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
