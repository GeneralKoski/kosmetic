//! Hand-written TOML lexer and lossless parser.
//!
//! Pipeline: [`lex`] turns the source into [`Token`]s, [`parse`] builds a
//! lossless tree that keeps whitespace and comments, so that rules can point
//! at exact positions and fixes can edit the file without reformatting it.
//!
//! Target: TOML v1.0.0 (<https://toml.io/en/v1.0.0>), checked against the
//! official `toml-test` suite.

mod lexer;
mod literal;
mod model;
mod parser;
mod token;
mod tree;

pub use lexer::lex;
pub use literal::{DateTimeKind, string_value};
pub use model::{DateTime, Table, TableOrigin, Value, build_model};
pub use token::{Token, TokenKind};
pub use tree::{Element, Node, NodeKind};

use kosmetic_diagnostics::Diagnostic;

/// Result of parsing a file: never fails, errors are reported as diagnostics.
#[derive(Debug, Clone)]
pub struct Parse {
    /// Every token of the file, trivia included, ending with [`TokenKind::Eof`].
    pub tokens: Vec<Token>,
    /// The lossless tree. Its leaves, in order, are exactly [`Parse::tokens`].
    pub tree: Node,
    /// Syntax errors found while lexing and parsing.
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses a TOML document. A single typo does not hide the other errors:
/// the parser recovers at the end of the line and goes on.
pub fn parse(source: &str) -> Parse {
    let (tokens, mut diagnostics) = lex(source);
    let tree = parser::parse(source, &tokens, &mut diagnostics);
    Parse {
        tokens,
        tree,
        diagnostics,
    }
}
