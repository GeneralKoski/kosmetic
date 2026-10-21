//! Hand-written TOML lexer and lossless parser.
//!
//! Pipeline: [`lex`] turns the source into [`Token`]s, [`parse`] builds a
//! lossless tree that keeps whitespace and comments, so that rules can point
//! at exact positions and fixes can edit the file without reformatting it.
//!
//! Target: TOML v1.0.0 (<https://toml.io/en/v1.0.0>), checked against the
//! official `toml-test` suite.

mod lexer;
mod token;

pub use lexer::lex;
pub use token::{Token, TokenKind};

use kosmetic_diagnostics::Diagnostic;

/// Result of parsing a file: never fails, errors are reported as diagnostics.
#[derive(Debug, Clone)]
pub struct Parse {
    /// Every token of the file, trivia included, ending with [`TokenKind::Eof`].
    pub tokens: Vec<Token>,
    /// Syntax errors found while lexing and parsing.
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses a TOML document.
///
/// TODO(M1): build the lossless tree (tables, array of tables, key/value
/// pairs, inline tables, arrays) with error recovery, so a single typo does
/// not hide every other error in the file.
pub fn parse(source: &str) -> Parse {
    let (tokens, diagnostics) = lex(source);
    Parse {
        tokens,
        diagnostics,
    }
}
