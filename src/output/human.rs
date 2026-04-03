use super::OutputFormatter;
use crossterm::style::Stylize;
use crossterm::tty::IsTty;
use fff::grep::GrepMatch;
use fff::types::{FileItem, Score};
use std::io::{self, Write};

pub struct HumanFormatter {
    use_color: bool,
    stdout: io::BufWriter<io::Stdout>,
}

impl HumanFormatter {
    pub fn new() -> Self {
        let use_color = io::stdout().is_tty()
            && std::env::var("NO_COLOR").is_err();
        Self {
            use_color,
            stdout: io::BufWriter::new(io::stdout()),
        }
    }

    fn colored_path(&self, relative_path: &str) -> String {
        if !self.use_color {
            return relative_path.to_string();
        }

        match relative_path.rfind('/') {
            Some(pos) => {
                let dir = &relative_path[..=pos];
                let name = &relative_path[pos + 1..];
                format!("{}{}", dir.dim(), name.bold())
            }
            None => format!("{}", relative_path.bold()),
        }
    }
}

impl OutputFormatter for HumanFormatter {
    fn print_file_match(&mut self, file: &FileItem, score: &Score, debug: bool) {
        let path = self.colored_path(&file.relative_path);
        if debug {
            let _ = writeln!(
                self.stdout,
                "{path}  [total={} base={} freq={} file={} git={} dist={} combo={}]",
                score.total,
                score.base_score,
                score.frecency_boost,
                score.filename_bonus,
                score.git_status_boost,
                score.distance_penalty,
                score.combo_match_boost,
            );
        } else {
            let _ = writeln!(self.stdout, "{path}");
        }
    }

    fn print_grep_match(&mut self, file: &FileItem, m: &GrepMatch) {
        let path = self.colored_path(&file.relative_path);

        if self.use_color {
            let _ = write!(
                self.stdout,
                "{}{}{}{}",
                path,
                ":".dim(),
                m.line_number.to_string().dim(),
                ":".dim(),
            );
        } else {
            let _ = write!(self.stdout, "{}:{}:", file.relative_path, m.line_number);
        }

        // Print line with match highlighting
        let line = m.line_content.trim_end();
        if self.use_color && !m.match_byte_offsets.is_empty() {
            let mut last = 0usize;
            for &(start, end) in &m.match_byte_offsets {
                let s = start as usize;
                let e = end as usize;
                if s > last && s <= line.len() {
                    let _ = write!(self.stdout, "{}", &line[last..s]);
                }
                if e <= line.len() {
                    let _ = write!(
                        self.stdout,
                        "{}",
                        &line[s..e].red().bold()
                    );
                    last = e;
                }
            }
            if last < line.len() {
                let _ = write!(self.stdout, "{}", &line[last..]);
            }
            let _ = writeln!(self.stdout);
        } else {
            let _ = writeln!(self.stdout, "{line}");
        }
    }

    fn flush(&mut self) {
        let _ = self.stdout.flush();
    }
}
