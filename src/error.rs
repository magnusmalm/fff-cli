use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum CliError {
    #[error("{0}")]
    Core(#[from] fff::Error),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("index not found at {0} — run `fff index` or search will auto-index")]
    NoIndex(PathBuf),

    #[error("corrupt index at {path}: {reason}")]
    CorruptIndex { path: PathBuf, reason: String },

    #[error("{0}")]
    Git(#[from] git2::Error),
}

pub type Result<T> = std::result::Result<T, CliError>;

/// Exit codes following ripgrep convention.
pub const EXIT_OK: i32 = 0;
pub const EXIT_NO_MATCH: i32 = 1;
pub const EXIT_ERROR: i32 = 2;
