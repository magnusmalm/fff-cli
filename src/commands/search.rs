use crate::error::{self, Result};
use crate::index;
use crate::output;
use fff::file_picker::FilePicker;
use fff::types::PaginationArgs;
use fff::FuzzySearchOptions;
use fff_query_parser::QueryParser;
use std::path::Path;

pub struct SearchOpts<'a> {
    pub query: &'a str,
    pub max_results: usize,
    pub json: bool,
    pub debug: bool,
    pub frecency_db: Option<&'a str>,
    pub git_repo: Option<&'a git2::Repository>,
}

pub fn run(project_root: &Path, opts: SearchOpts<'_>) -> Result<i32> {
    let loaded = index::ensure_index(project_root, opts.git_repo, opts.frecency_db)?;

    let parser = QueryParser::default();
    let parsed = parser.parse(opts.query);

    let result = FilePicker::fuzzy_search(
        &loaded.files,
        &parsed,
        None, // no query tracker for now
        FuzzySearchOptions {
            max_threads: 0,
            current_file: None,
            project_path: Some(project_root),
            pagination: PaginationArgs {
                offset: 0,
                limit: opts.max_results,
            },
            ..Default::default()
        },
    );

    if result.items.is_empty() {
        return Ok(error::EXIT_NO_MATCH);
    }

    let mut fmt = output::formatter(opts.json);
    for (file, score) in result.items.iter().zip(result.scores.iter()) {
        fmt.print_file_match(file, score, opts.debug);
    }
    fmt.flush();

    Ok(error::EXIT_OK)
}
