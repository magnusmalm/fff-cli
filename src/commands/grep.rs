use crate::error::{self, Result};
use crate::index;
use crate::output;
use fff::grep::{grep_search, GrepMode, GrepSearchOptions};
use fff::types::ContentCacheBudget;
use fff_query_parser::{GrepConfig, QueryParser};
use std::path::Path;

pub struct GrepOpts<'a> {
    pub pattern: &'a str,
    pub regex: bool,
    pub fuzzy: bool,
    pub max_results: usize,
    pub json: bool,
    pub before_context: usize,
    pub after_context: usize,
    pub frecency_db: Option<&'a str>,
    pub git_repo: Option<&'a git2::Repository>,
}

pub fn run(project_root: &Path, opts: GrepOpts<'_>) -> Result<i32> {
    let loaded = index::ensure_index(project_root, opts.git_repo, opts.frecency_db)?;

    let mode = if opts.regex {
        GrepMode::Regex
    } else if opts.fuzzy {
        GrepMode::Fuzzy
    } else {
        GrepMode::PlainText
    };

    let parser = QueryParser::new(GrepConfig);
    let parsed = parser.parse(opts.pattern);

    let budget = ContentCacheBudget::new_for_repo(loaded.files.len());

    let grep_options = GrepSearchOptions {
        max_file_size: 10 * 1024 * 1024,
        max_matches_per_file: 100,
        smart_case: true,
        file_offset: 0,
        page_limit: opts.max_results,
        mode,
        time_budget_ms: 0,
        before_context: opts.before_context,
        after_context: opts.after_context,
        classify_definitions: true,
    };

    let result = grep_search(
        &loaded.files,
        &parsed,
        &grep_options,
        &budget,
        loaded.bigram.as_ref(),
        None, // no overlay
        None, // no cancellation
    );

    if result.matches.is_empty() {
        return Ok(error::EXIT_NO_MATCH);
    }

    let mut fmt = output::formatter(opts.json);
    for m in &result.matches {
        let file = result.files[m.file_index];
        fmt.print_grep_match(file, m);
    }
    fmt.flush();

    Ok(error::EXIT_OK)
}
