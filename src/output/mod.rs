pub mod human;
pub mod json;

use fff::types::{FileItem, Score};
use fff::grep::GrepMatch;

pub trait OutputFormatter {
    fn print_file_match(&mut self, file: &FileItem, score: &Score, debug: bool);
    fn print_grep_match(&mut self, file: &FileItem, m: &GrepMatch);
    fn flush(&mut self);
}

/// Create the appropriate formatter based on CLI flags.
pub fn formatter(json: bool) -> Box<dyn OutputFormatter> {
    if json {
        Box::new(json::JsonFormatter::new())
    } else {
        Box::new(human::HumanFormatter::new())
    }
}
