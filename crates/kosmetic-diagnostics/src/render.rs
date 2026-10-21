use std::fmt::Write as _;

use serde::Serialize;

use crate::{Diagnostic, LineIndex};

/// Renders diagnostics in a compact human format:
///
/// ```text
/// Cargo.toml:3:7: warning[style/no-trailing-whitespace]: trailing whitespace
///   3 | name = "kosmetic"
///     |       ^^^
///   = help: remove the spaces at the end of the line
/// ```
///
/// TODO(M2): multi-line spans, secondary labels and colors.
pub fn render_human(path: &str, source: &str, diagnostics: &[Diagnostic]) -> String {
    let index = LineIndex::new(source);
    let mut out = String::new();

    for diagnostic in diagnostics {
        let start = index.line_col(diagnostic.span.start);
        let _ = writeln!(
            out,
            "{path}:{}:{}: {}[{}]: {}",
            start.line,
            start.col,
            diagnostic.severity.as_str(),
            diagnostic.code,
            diagnostic.message,
        );

        if let Some(text) = index.line_text(start.line) {
            let gutter = start.line.to_string();
            let pad = " ".repeat(gutter.len());
            let end = index.line_col(diagnostic.span.end);
            let width = if end.line == start.line {
                (end.col - start.col).max(1)
            } else {
                1
            };
            let _ = writeln!(out, "  {gutter} | {text}");
            let _ = writeln!(
                out,
                "  {pad} | {}{}",
                " ".repeat(start.col as usize - 1),
                "^".repeat(width as usize),
            );
        }

        if let Some(help) = &diagnostic.help {
            let _ = writeln!(out, "  = help: {help}");
        }
    }

    out
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

    #[test]
    fn human_output_points_at_the_span() {
        let source = "name = \"x\"   \n";
        let diagnostic = Diagnostic::new(
            "style/no-trailing-whitespace",
            Severity::Warning,
            "trailing whitespace",
            Span::new(10, 13),
        );
        let out = render_human("Cargo.toml", source, &[diagnostic]);
        assert!(out.starts_with("Cargo.toml:1:11: warning[style/no-trailing-whitespace]"));
        assert!(out.contains("|           ^^^"));
    }
}
