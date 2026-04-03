use super::OutputFormatter;
use fff::grep::GrepMatch;
use fff::types::{FileItem, Score};
use std::io::{self, Write};

pub struct JsonFormatter {
    stdout: io::BufWriter<io::Stdout>,
}

impl JsonFormatter {
    pub fn new() -> Self {
        Self {
            stdout: io::BufWriter::new(io::stdout()),
        }
    }
}

impl OutputFormatter for JsonFormatter {
    fn print_file_match(&mut self, file: &FileItem, score: &Score, _debug: bool) {
        let obj = serde_json::json!({
            "path": file.relative_path,
            "score": score.total,
            "match_type": score.match_type,
            "frecency": score.frecency_boost,
        });
        let _ = writeln!(self.stdout, "{}", obj);
    }

    fn print_grep_match(&mut self, file: &FileItem, m: &GrepMatch) {
        let obj = serde_json::json!({
            "path": file.relative_path,
            "line_number": m.line_number,
            "column": m.col,
            "line": m.line_content.trim_end(),
            "byte_offset": m.byte_offset,
            "is_definition": m.is_definition,
        });
        let _ = writeln!(self.stdout, "{}", obj);
    }

    fn flush(&mut self) {
        let _ = self.stdout.flush();
    }
}
