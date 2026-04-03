mod cli;
mod commands;
mod error;
mod index;
mod output;

use clap::Parser;
use cli::{Cli, Commands};
use mimalloc::MiMalloc;


#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    let cli = Cli::parse();

    // Initialize tracing from RUST_LOG or default to warn.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let exit_code = match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            error::EXIT_ERROR
        }
    };

    std::process::exit(exit_code);
}

fn run(cli: Cli) -> error::Result<i32> {
    // Resolve project root (single git discovery for the whole invocation).
    let cwd = std::env::current_dir()?;
    let start_dir = cli.directory.as_deref().unwrap_or(&cwd);
    let (project_root, git_repo) = index::resolve_project_root(start_dir);

    // Resolve frecency database path.
    let frecency_db = index::resolve_frecency_db(
        cli.frecency_db.as_deref(),
        &project_root,
    );
    let frecency_db_ref = frecency_db.as_deref();

    // Dispatch subcommand (or implicit search).
    match cli.command {
        Some(Commands::Index { path, force }) => {
            let root = path
                .map(|p| {
                    if p.is_absolute() {
                        p
                    } else {
                        cwd.join(p)
                    }
                })
                .unwrap_or_else(|| project_root.clone());
            commands::index::run(&root, force)?;
            Ok(error::EXIT_OK)
        }

        Some(Commands::Search { ref query }) => commands::search::run(
            &project_root,
            commands::search::SearchOpts {
                query,
                max_results: cli.max_results,
                json: cli.json,
                debug: cli.debug,
                frecency_db: frecency_db_ref,
                git_repo: git_repo.as_ref(),
            },
        ),

        Some(Commands::Grep {
            ref pattern,
            regex,
            fuzzy,
            before_context,
            after_context,
            context,
        }) => {
            let ctx = context.unwrap_or(0);
            commands::grep::run(
                &project_root,
                commands::grep::GrepOpts {
                    pattern,
                    regex,
                    fuzzy,
                    max_results: cli.max_results,
                    json: cli.json,
                    before_context: before_context.unwrap_or(ctx),
                    after_context: after_context.unwrap_or(ctx),
                    frecency_db: frecency_db_ref,
                    git_repo: git_repo.as_ref(),
                },
            )
        }

        Some(Commands::Filter { ref query }) => {
            commands::filter::run(query, cli.max_results)
        }

        Some(Commands::Watch) => {
            commands::watch::run(&project_root)?;
            Ok(error::EXIT_OK)
        }

        Some(Commands::Completions { shell }) => {
            use clap::CommandFactory;
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "fff",
                &mut std::io::stdout(),
            );
            Ok(error::EXIT_OK)
        }

        // No subcommand: treat positional argument as search query.
        None => {
            if let Some(ref query) = cli.implicit_query {
                commands::search::run(
                    &project_root,
                    commands::search::SearchOpts {
                        query,
                        max_results: cli.max_results,
                        json: cli.json,
                        debug: cli.debug,
                        frecency_db: frecency_db_ref,
                        git_repo: git_repo.as_ref(),
                    },
                )
            } else {
                // No query at all — print help.
                use clap::CommandFactory;
                Cli::command().print_help()?;
                println!();
                Ok(error::EXIT_OK)
            }
        }
    }
}
