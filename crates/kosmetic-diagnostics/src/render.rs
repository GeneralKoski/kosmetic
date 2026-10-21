use std::fmt::Write as _;

use serde::Serialize;

use crate::{Diagnostic, LineCol, LineIndex, Severity, Span};

/// Renders diagnostics for a terminal, with the source lines they point at:
///
/// ```text
/// Cargo.toml:3:1: error[semantic/duplicate-key]: duplicate key `name`
///   2 | name = "a"
///     | ---- first defined here
///   3 | name = "b"
///     | ^^^^
///   = help: remove one of the two definitions
/// ```
///
/// The first line stays `path:line:col:` so editors and `grep` can jump to it.
/// The main span is marked with `^`, labels with `-` and their message; a
/// span over several lines marks each of them, eliding the middle of long
/// ones. With `color`, severities, markers and the gutter use ANSI colors.
pub fn render_human(path: &str, source: &str, diagnostics: &[Diagnostic], color: bool) -> String {
    let index = LineIndex::new(source);
    let paint = |style: &str, text: &str| {
        if color {
            format!("\x1b[{style}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    };
    let mut out = String::new();

    for diagnostic in diagnostics {
        let severity_style = match diagnostic.severity {
            Severity::Error => "1;31",
            Severity::Warning => "1;33",
            Severity::Info => "1;36",
        };
        let start = index.line_col(diagnostic.span.start);
        let _ = writeln!(
            out,
            "{path}:{}:{}: {}: {}",
            start.line,
            start.col,
            paint(
                severity_style,
                &format!("{}[{}]", diagnostic.severity.as_str(), diagnostic.code)
            ),
            paint("1", &diagnostic.message),
        );

        let mut marks = vec![Mark::new(&index, diagnostic.span, '^', None)];
        marks.extend(
            diagnostic
                .labels
                .iter()
                .map(|label| Mark::new(&index, label.span, '-', Some(&label.message))),
        );
        let mut lines: Vec<u32> = marks.iter().flat_map(Mark::shown_lines).collect();
        lines.sort_unstable();
        lines.dedup();
        let width = lines.last().map_or(1, |line| line.to_string().len());
        let gutter = |text: &str| paint("1;34", &format!("  {text:>width$} |"));

        let mut previous = None;
        for line in lines {
            let Some(text) = index.line_text(line) else {
                continue;
            };
            if previous.is_some_and(|previous| line > previous + 1) {
                let _ = writeln!(out, "{}", paint("1;34", &format!("  {:>width$}", "...")));
            }
            previous = Some(line);
            let _ = writeln!(out, "{} {}", gutter(&line.to_string()), expand_tabs(text));

            for mark in marks.iter().filter(|mark| mark.covers(line)) {
                let from = if line == mark.start.line {
                    display_width(text, mark.start.col)
                } else {
                    0
                };
                let to = if line == mark.end.line {
                    display_width(text, mark.end.col)
                } else {
                    display_width(text, u32::MAX)
                };
                let underline = mark
                    .symbol
                    .to_string()
                    .repeat(to.saturating_sub(from).max(1));
                let style = if mark.symbol == '^' {
                    severity_style
                } else {
                    "1;34"
                };
                let mut marker = format!("{}{underline}", " ".repeat(from));
                if let (Some(message), true) = (mark.message, line == mark.end.line) {
                    marker = format!("{marker} {message}");
                }
                let _ = writeln!(out, "{} {}", gutter(""), paint(style, &marker));
            }
        }

        if let Some(help) = &diagnostic.help {
            let _ = writeln!(out, "  {} {help}", paint("1;36", "= help:"));
        }
        out.push('\n');
    }

    out
}

/// One underlined span: the main one or a label.
struct Mark<'a> {
    start: LineCol,
    end: LineCol,
    symbol: char,
    message: Option<&'a str>,
}

impl<'a> Mark<'a> {
    fn new(index: &LineIndex<'_>, span: Span, symbol: char, message: Option<&'a str>) -> Self {
        let start = index.line_col(span.start);
        let mut end = index.line_col(span.end);
        // A span that ends with a newline stops on the line before, not at column 1 of the next.
        if end.line > start.line && end.col == 1 {
            end.line -= 1;
            end.col = index.line_text(end.line).map_or(1, |text| {
                u32::try_from(text.chars().count() + 1).unwrap_or(u32::MAX)
            });
        }
        Self {
            start,
            end,
            symbol,
            message,
        }
    }

    fn covers(&self, line: u32) -> bool {
        (self.start.line..=self.end.line).contains(&line) && self.shown_lines().contains(&line)
    }

    /// All lines of short spans; the first two and last two of long ones.
    fn shown_lines(&self) -> Vec<u32> {
        let (first, last) = (self.start.line, self.end.line);
        if last - first < 4 {
            (first..=last).collect()
        } else {
            vec![first, first + 1, last - 1, last]
        }
    }
}

const TAB_WIDTH: usize = 4;

fn expand_tabs(text: &str) -> String {
    text.replace('\t', &" ".repeat(TAB_WIDTH))
}

/// Terminal columns taken by the characters before the 1-based column `col`.
fn display_width(text: &str, col: u32) -> usize {
    text.chars()
        .take(col.saturating_sub(1) as usize)
        .map(|c| if c == '\t' { TAB_WIDTH } else { 1 })
        .sum()
}

#[derive(Serialize)]
struct JsonDiagnostic<'a> {
    path: &'a str,
    line: u32,
    column: u32,
    #[serde(flatten)]
    diagnostic: &'a Diagnostic,
}

/// Renders diagnostics as a JSON array, one object per diagnostic, for CI and editors.
///
/// # Panics
///
/// Never in practice: every field is plain data that `serde_json` can serialize.
pub fn render_json(path: &str, source: &str, diagnostics: &[Diagnostic]) -> String {
    let index = LineIndex::new(source);
    let items: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| {
            let start = index.line_col(diagnostic.span.start);
            JsonDiagnostic {
                path,
                line: start.line,
                column: start.col,
                diagnostic,
            }
        })
        .collect();
    serde_json::to_string_pretty(&items).expect("diagnostics are always serializable")
}

#[cfg(test)]
mod tests {
    use super::render_human;
    use crate::{Diagnostic, Severity, Span};

    fn render(source: &str, diagnostic: Diagnostic) -> String {
        render_human("f.toml", source, &[diagnostic], false)
    }

    #[test]
    fn human_output_points_at_the_span() {
        let source = "name = \"x\"   \n";
        let diagnostic = Diagnostic::new(
            "style/no-trailing-whitespace",
            Severity::Warning,
            "trailing whitespace",
            Span::new(10, 13),
        )
        .with_help("remove them");
        let expected = "\
f.toml:1:11: warning[style/no-trailing-whitespace]: trailing whitespace
  1 | name = \"x\"   
    |           ^^^
  = help: remove them

";
        assert_eq!(render(source, diagnostic), expected);
    }

    #[test]
    fn labels_are_shown_with_their_message() {
        let source = "a = 1\nb = 2\na = 3\n";
        let diagnostic = Diagnostic::new(
            "semantic/duplicate-key",
            Severity::Error,
            "duplicate key `a`",
            Span::new(12, 13),
        )
        .with_label(Span::new(0, 1), "first defined here");
        let expected = "\
f.toml:3:1: error[semantic/duplicate-key]: duplicate key `a`
  1 | a = 1
    | - first defined here
  ...
  3 | a = 3
    | ^

";
        assert_eq!(render(source, diagnostic), expected);
    }

    #[test]
    fn multi_line_spans_mark_every_line_and_elide_long_ones() {
        let source = "s = \"\"\"one\ntwo\nthree\nfour\nfive\"\"\"\n";
        let end = u32::try_from(source.len() - 1).expect("small");
        let diagnostic = Diagnostic::new("x/y", Severity::Error, "long", Span::new(4, end));
        let expected = "\
f.toml:1:5: error[x/y]: long
  1 | s = \"\"\"one
    |     ^^^^^^
  2 | two
    | ^^^
  ...
  4 | four
    | ^^^^
  5 | five\"\"\"
    | ^^^^^^^

";
        assert_eq!(render(source, diagnostic), expected);
    }

    #[test]
    fn tabs_keep_markers_aligned() {
        let diagnostic = Diagnostic::new("x/y", Severity::Info, "here", Span::new(2, 3));
        assert!(render("\ta=1", diagnostic).contains("  1 |     a=1\n    |      ^\n"));
    }

    #[test]
    fn colors_are_optional() {
        let diagnostic = Diagnostic::new("x/y", Severity::Error, "boom", Span::new(0, 1));
        assert!(!render("a", diagnostic.clone()).contains('\x1b'));
        let colored = render_human("f.toml", "a", &[diagnostic], true);
        assert!(colored.contains("\x1b[1;31merror[x/y]\x1b[0m"));
    }
}
