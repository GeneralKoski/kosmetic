//! TOML formatter built on the lossless tree.
//!
//! The formatter only changes layout: spaces, indentation, blank lines and
//! the trailing comma of multi-line arrays. Comments are kept, keys and
//! values are never rewritten, and the result is checked to hold exactly the
//! same values as the input.
//!
//! Layout, in short:
//!
//! - `key = value`, `a.b.c`, `[table]`, `[[array.of.tables]]`: no stray spaces
//! - `[1, 2, 3]` and `{ a = 1, b = 2 }`, with `{}` and `[]` for empty ones
//! - multi-line arrays: items indented by two spaces per level, a trailing
//!   comma after the last item, the closing bracket on its own line
//! - no indentation and no trailing whitespace anywhere else
//! - at most one blank line in a row, exactly one before every table header
//!   (or before the comments right above it), none at the start of the file
//! - one line ending style, the one the file starts with, and exactly one
//!   final newline

mod layout;
mod lines;

use kosmetic_syntax::{Table, TokenKind, build_model, parse};

/// Why a file was not formatted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    /// The file has syntax or semantic errors: only valid documents are formatted.
    Invalid,
    /// The result would not hold the same values or comments as the input.
    /// This is a bug in the formatter, the file is left untouched.
    ChangesMeaning,
}

/// The values of a document, or `None` if it has any error.
fn model(source: &str) -> Option<Table> {
    let parse = parse(source);
    let (table, semantic) = build_model(source, &parse.tree);
    (parse.diagnostics.is_empty() && semantic.is_empty()).then_some(table)
}

/// Comment texts without trailing whitespace, in order.
fn comments(source: &str) -> Vec<String> {
    parse(source)
        .tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Comment)
        .map(|token| {
            source[token.span.range()]
                .trim_end_matches([' ', '\t'])
                .to_owned()
        })
        .collect()
}

/// Formats a valid TOML document.
///
/// # Errors
///
/// [`FormatError::Invalid`] if the document has errors,
/// [`FormatError::ChangesMeaning`] if the output would differ in values or
/// comments from the input.
pub fn format(source: &str) -> Result<String, FormatError> {
    let original = model(source).ok_or(FormatError::Invalid)?;
    let formatted = lines::arrange(&layout::layout(source));
    if model(&formatted).as_ref() != Some(&original) || comments(&formatted) != comments(source) {
        return Err(FormatError::ChangesMeaning);
    }
    Ok(formatted)
}

#[cfg(test)]
mod tests;
