mod cli;
mod mcp;

use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::cli::{Cli, Commands, DepsCommands, QueryCommands, TagsCommands};

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
        Commands::Index {
            path,
            watch,
            deep_deps,
            durability,
            profile,
            threads,
            throttle_ms,
        } => {
            cli::index_directory(
                &path,
                &cli.db,
                watch,
                durability,
                profile,
                threads,
                throttle_ms,
            )?;
            if deep_deps {
                cli::index_dependencies(&path, &cli.db, None, false)?;
            }
        }
        Commands::Serve { transport, socket } => {
            cli::run_mcp_server(&cli.db, transport, socket.as_deref()).await?;
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
            remote,
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
                remote.as_deref(),
            )
            .await?;
        }
        Commands::Definition {
            name,
            include_deps,
            dep,
            remote,
        } => {
            cli::find_definition(&cli.db, &name, include_deps, dep, remote.as_deref()).await?;
        }
        Commands::References {
            name,
            callers,
            depth,
            file,
            limit,
            remote,
        } => {
            cli::find_references(
                &cli.db,
                &name,
                callers,
                depth,
                file,
                limit,
                remote.as_deref(),
            )
            .await?;
        }
        Commands::CallGraph {
            function,
            direction,
            depth,
            include_possible,
            remote,
        } => {
            cli::analyze_call_graph(
                &cli.db,
                &function,
                &direction,
                depth,
                include_possible,
                remote.as_deref(),
            )
            .await?;
        }
        Commands::Outline {
            file,
            start_line,
            end_line,
            scopes,
            remote,
        } => {
            cli::get_outline(
                &cli.db,
                &file,
                start_line,
                end_line,
                scopes,
                remote.as_deref(),
            )
            .await?;
        }
        Commands::Imports {
            file,
            resolve,
            remote,
        } => {
            cli::get_imports(&cli.db, &file, resolve, remote.as_deref()).await?;
        }
        Commands::Changed {
            base,
            staged,
            unstaged,
            format,
        } => {
            cli::get_changed_symbols(&cli.db, &base, staged, unstaged, &format)?;
        }
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
            provider,
            model,
            endpoint,
            format,
            remote,
        } => {
            cli::prepare_context(
                &cli.db,
                &query,
                file,
                line,
                column,
                task_hint,
                max_items,
                approx_tokens,
                include_snippets,
                snippet_lines,
                provider,
                model,
                endpoint,
                &format,
                remote.as_deref(),
            )
            .await?;
        }
        Commands::Stats { remote } => {
            cli::show_stats(&cli.db, remote.as_deref()).await?;
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
        Commands::Tags { command } => match command {
            TagsCommands::AddRule {
                tag,
                pattern,
                confidence,
                path,
            } => {
                cli::add_tag_rule(&path, &tag, &pattern, confidence)?;
            }
            TagsCommands::RemoveRule { pattern, path } => {
                cli::remove_tag_rule(&path, &pattern)?;
            }
            TagsCommands::ListRules { path, format } => {
                cli::list_tag_rules(&path, &format)?;
            }
            TagsCommands::Preview { file, path } => {
                cli::preview_tags(&file, &path)?;
            }
            TagsCommands::Apply { path, db } => {
                cli::apply_tags(&path, &db)?;
            }
            TagsCommands::Stats { db } => {
                cli::show_tag_stats(&db)?;
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
                    cli::find_definition(&cli.db, &name, false, None, None).await?;
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
