mod cli;
mod mcp;

use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::cli::{Cli, Commands, DepsCommands, QueryCommands};

// Re-export from lib for internal use
use code_indexer::{error, index, indexer, languages};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "code_indexer=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    let cli = Cli::parse();

    match cli.command {
        // === New consolidated commands ===
        Commands::Index { path, watch, deep_deps } => {
            cli::index_directory(&path, &cli.db, watch)?;
            if deep_deps {
                cli::index_dependencies(&path, &cli.db, None, false)?;
            }
        }
        Commands::Serve => {
            cli::run_mcp_server(&cli.db).await?;
        }
        Commands::Symbols {
            query,
            kind,
            limit,
            language,
            file,
            pattern,
            format,
            fuzzy,
            fuzzy_threshold,
        } => {
            cli::symbols(
                &cli.db,
                query,
                &kind,
                limit,
                language,
                file,
                pattern,
                &format,
                fuzzy,
                fuzzy_threshold,
            )?;
        }
        Commands::Definition { name, include_deps, dep } => {
            if include_deps {
                cli::find_in_dependencies(&cli.db, &name, dep, 20)?;
            } else {
                cli::find_definition(&cli.db, &name)?;
            }
        }
        Commands::References {
            name,
            callers,
            depth,
            file,
            limit,
        } => {
            cli::find_references(&cli.db, &name, callers, depth, file, limit)?;
        }
        Commands::CallGraph {
            function,
            direction,
            depth,
            include_possible,
        } => {
            cli::analyze_call_graph(&cli.db, &function, &direction, depth, include_possible)?;
        }
        Commands::Outline {
            file,
            start_line,
            end_line,
            scopes,
        } => {
            cli::get_outline(&cli.db, &file, start_line, end_line, scopes)?;
        }
        Commands::Imports { file, resolve } => {
            cli::get_imports(&cli.db, &file, resolve)?;
        }
        Commands::Changed {
            base,
            staged,
            unstaged,
            format,
        } => {
            cli::get_changed_symbols(&cli.db, &base, staged, unstaged, &format)?;
        }
        Commands::Stats => {
            cli::show_stats(&cli.db)?;
        }
        Commands::Clear => {
            cli::clear_index(&cli.db)?;
        }
        Commands::Deps { command } => match command {
            DepsCommands::List { path, dev, format } => {
                cli::list_dependencies(&path, &cli.db, dev, &format)?;
            }
            DepsCommands::Index { path, name, dev } => {
                cli::index_dependencies(&path, &cli.db, name, dev)?;
            }
            DepsCommands::Find { name, dep, limit } => {
                cli::find_in_dependencies(&cli.db, &name, dep, limit)?;
            }
            DepsCommands::Source { name, dep, context } => {
                cli::get_dependency_source(&cli.db, &name, dep, context)?;
            }
            DepsCommands::Info { name, path } => {
                cli::show_dependency_info(&path, &cli.db, &name)?;
            }
        },
        // === Legacy commands (deprecated) ===
        Commands::Query { query } => {
            eprintln!("Warning: 'query' command is deprecated. Use 'symbols', 'definition', or 'references' instead.");
            match query {
                QueryCommands::Search {
                    query,
                    limit,
                    format,
                    fuzzy,
                    fuzzy_threshold,
                } => {
                    cli::search_symbols(&cli.db, &query, limit, &format, fuzzy, fuzzy_threshold)?;
                }
                QueryCommands::Definition { name } => {
                    cli::find_definition(&cli.db, &name)?;
                }
                QueryCommands::Functions {
                    limit,
                    language,
                    file,
                    pattern,
                    format,
                } => {
                    cli::list_functions(&cli.db, limit, language, file, pattern, &format)?;
                }
                QueryCommands::Types {
                    limit,
                    language,
                    file,
                    pattern,
                    format,
                } => {
                    cli::list_types(&cli.db, limit, language, file, pattern, &format)?;
                }
                QueryCommands::Changed {
                    base,
                    staged,
                    unstaged,
                    format,
                } => {
                    cli::get_changed_symbols(&cli.db, &base, staged, unstaged, &format)?;
                }
            }
        }
    }

    Ok(())
}
