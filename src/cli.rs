use clap::{Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "fff",
    version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("FFF_GIT_HASH"), ")"),
    about = "Fast file finder and grep",
    after_help = "When no subcommand is given, the first positional argument is treated as a fuzzy file search query."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Project root (defaults to git root or cwd).
    #[arg(short = 'C', long, global = true)]
    pub directory: Option<PathBuf>,

    /// Output JSON (NDJSON) instead of human-readable format.
    #[arg(long, global = true)]
    pub json: bool,

    /// Show scores and debug info.
    #[arg(long, global = true)]
    pub debug: bool,

    /// Maximum results to return.
    #[arg(short = 'n', long, global = true, default_value = "50")]
    pub max_results: usize,

    /// Path to frecency database. Auto-detected from nvim cache or .fff/.
    #[arg(long, global = true, env = "FFF_FRECENCY_DB")]
    pub frecency_db: Option<String>,

    /// Bare positional: treated as `fff search <query>` when no subcommand.
    #[arg(value_name = "QUERY", hide = true)]
    pub implicit_query: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Build file list and bigram index.
    Index {
        /// Path to index (defaults to project root).
        path: Option<PathBuf>,
        /// Force full rebuild even if index exists.
        #[arg(long)]
        force: bool,
    },
    /// Fuzzy file search by name.
    Search {
        /// Fuzzy query string (supports constraints like `git:modified`, `src/`, `*.rs`).
        query: String,
    },
    /// Search file contents.
    Grep {
        /// Search pattern.
        pattern: String,
        /// Use regex mode.
        #[arg(short = 'e', long)]
        regex: bool,
        /// Use fuzzy mode.
        #[arg(long)]
        fuzzy: bool,
        /// Context lines before each match.
        #[arg(short = 'B', long)]
        before_context: Option<usize>,
        /// Context lines after each match.
        #[arg(short = 'A', long)]
        after_context: Option<usize>,
        /// Context lines before and after each match.
        #[arg(long)]
        context: Option<usize>,
    },
    /// Fuzzy filter lines from stdin (like fzf --filter). No index needed.
    #[command(name = "--filter", alias = "filter")]
    Filter {
        /// Fuzzy query string.
        query: String,
    },
    /// Watch filesystem and keep index updated.
    Watch,
    /// Generate shell completions.
    Completions {
        /// Shell to generate completions for.
        shell: Shell,
    },
}
