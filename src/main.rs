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
        Commands::Index { path, watch } => {
            cli::index_directory(&path, &cli.db, watch)?;
        }
        Commands::Serve => {
            cli::run_mcp_server(&cli.db).await?;
        }
        Commands::Query { query } => match query {
            QueryCommands::Search { query, limit } => {
                cli::search_symbols(&cli.db, &query, limit)?;
            }
            QueryCommands::Definition { name } => {
                cli::find_definition(&cli.db, &name)?;
            }
            QueryCommands::Functions {
                limit,
                language,
                file,
                pattern,
            } => {
                cli::list_functions(&cli.db, limit, language, file, pattern)?;
            }
            QueryCommands::Types {
                limit,
                language,
                file,
                pattern,
            } => {
                cli::list_types(&cli.db, limit, language, file, pattern)?;
            }
        },
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
    }

    Ok(())
}
