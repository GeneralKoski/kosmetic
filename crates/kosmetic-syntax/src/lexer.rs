use kosmetic_diagnostics::{Diagnostic, Span};

use crate::{Token, TokenKind};

/// Splits `source` into tokens. Never fails: invalid input becomes
/// [`TokenKind::Error`] tokens plus diagnostics, and the token list always ends
/// with [`TokenKind::Eof`].
///
/// Invariant to keep: the spans of all tokens, in order, cover the whole source
/// without gaps or overlaps. That is what makes the tree lossless.
///
/// TODO(M1): implement the lexer. Suggested shape: a `Cursor` over the bytes
/// with `peek`, `bump` and `eat_while`, and one method per token family
/// (trivia, punctuation, the four string kinds, bare). Check every case of
/// the `toml-test` "invalid" suite that is a lexing error, for example control
/// characters in comments and unterminated strings.
///
/// # Panics
///
/// Panics if `source` is larger than `u32::MAX` bytes.
pub fn lex(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let len = u32::try_from(source.len()).expect("source larger than 4 GiB");
    let tokens = vec![Token {
        kind: TokenKind::Eof,
        span: Span::empty(len),
    }];
    (tokens, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::lex;
    use crate::TokenKind;

    #[test]
    fn always_ends_with_eof() {
        let (tokens, _) = lex("");
        assert_eq!(tokens.last().map(|token| token.kind), Some(TokenKind::Eof));
    }

    // TODO(M1): when the lexer exists, add a property test: for any input,
    // concatenating the token slices gives back the original source.
}
