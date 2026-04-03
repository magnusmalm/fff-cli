use crate::error::{self, Result};
use std::io::{self, BufRead, Write};

pub fn run(query: &str, max_results: usize) -> Result<i32> {
    let stdin = io::stdin().lock();
    let mut stdout = io::BufWriter::new(io::stdout().lock());

    // Collect lines from stdin.
    let lines: Vec<String> = stdin.lines().map_while(|l| l.ok()).collect();
    let haystack: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();

    if haystack.is_empty() || query.is_empty() {
        // No query: pass through up to max_results (like fzf --filter with empty query).
        for line in haystack.iter().take(max_results) {
            let _ = writeln!(stdout, "{line}");
        }
        return Ok(if haystack.is_empty() {
            error::EXIT_NO_MATCH
        } else {
            error::EXIT_OK
        });
    }

    let config = neo_frizbee::Config {
        max_typos: Some((query.len() as u16 / 4).clamp(2, 6)),
        sort: true,
        ..Default::default()
    };

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let matches = neo_frizbee::match_list_parallel(query, &haystack, &config, threads);

    if matches.is_empty() {
        return Ok(error::EXIT_NO_MATCH);
    }

    for m in matches.iter().take(max_results) {
        let _ = writeln!(stdout, "{}", haystack[m.index as usize]);
    }

    Ok(error::EXIT_OK)
}
