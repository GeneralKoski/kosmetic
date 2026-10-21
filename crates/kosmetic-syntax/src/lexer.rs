use kosmetic_diagnostics::{Diagnostic, Severity, Span};

use crate::{Token, TokenKind};

/// Splits `source` into tokens. Never fails: invalid input becomes
/// [`TokenKind::Error`] tokens plus diagnostics, and the token list always ends
/// with [`TokenKind::Eof`].
///
/// Invariant to keep: the spans of all tokens, in order, cover the whole source
/// without gaps or overlaps. That is what makes the tree lossless.
///
/// Strings that are unterminated or contain bad escapes keep their string kind,
/// so the parser can go on, but always come with a diagnostic.
///
/// # Panics
///
/// Panics if `source` is larger than `u32::MAX` bytes.
pub fn lex(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let len = u32::try_from(source.len()).expect("source larger than 4 GiB");
    let mut lexer = Lexer {
        cursor: Cursor {
            bytes: source.as_bytes(),
            pos: 0,
        },
        source,
        tokens: Vec::new(),
        diagnostics: Vec::new(),
    };
    while lexer.cursor.peek().is_some() {
        let start = lexer.cursor.pos;
        let kind = lexer.token(start);
        let span = span(start, lexer.cursor.pos);
        lexer.tokens.push(Token { kind, span });
    }
    lexer.tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::empty(len),
    });
    (lexer.tokens, lexer.diagnostics)
}

/// Byte offsets fit in `u32`: [`lex`] rejects larger sources up front.
#[allow(clippy::cast_possible_truncation)]
fn span(start: usize, end: usize) -> Span {
    Span::new(start as u32, end as u32)
}

/// Characters allowed in bare keys, plus `+` and `:` so that numbers and
/// date-times lex as a single [`TokenKind::Bare`]. `.` is always a separate
/// [`TokenKind::Dot`]: the parser glues `3`, `.`, `14` back into a float.
fn is_bare(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+' | b':')
}

/// Control characters TOML forbids in comments and strings. Callers handle
/// `\n` and `\r\n` before asking.
fn is_control(byte: u8) -> bool {
    (byte < 0x20 && byte != b'\t') || byte == 0x7F
}

fn is_token_start(byte: u8) -> bool {
    is_bare(byte)
        || matches!(
            byte,
            b' ' | b'\t'
                | b'\n'
                | b'\r'
                | b'#'
                | b'='
                | b'.'
                | b','
                | b'['
                | b']'
                | b'{'
                | b'}'
                | b'"'
                | b'\''
        )
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_at(&self, n: usize) -> Option<u8> {
        self.bytes.get(self.pos + n).copied()
    }

    fn bump(&mut self) {
        self.pos += 1;
    }

    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        if found {
            self.bump();
        }
        found
    }

    fn eat_while(&mut self, predicate: impl Fn(u8) -> bool) {
        while self.peek().is_some_and(&predicate) {
            self.bump();
        }
    }

    /// Whether a line ending (`\n` or `\r\n`) starts here.
    fn at_newline(&self) -> bool {
        self.peek() == Some(b'\n') || (self.peek() == Some(b'\r') && self.peek_at(1) == Some(b'\n'))
    }
}

struct Lexer<'a> {
    cursor: Cursor<'a>,
    source: &'a str,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl Lexer<'_> {
    fn error(&mut self, code: &'static str, message: impl Into<String>, start: usize, end: usize) {
        self.diagnostics.push(Diagnostic::new(
            code,
            Severity::Error,
            message,
            span(start, end),
        ));
    }

    fn token(&mut self, start: usize) -> TokenKind {
        if self.cursor.at_newline() {
            self.cursor.eat(b'\r');
            self.cursor.bump();
            return TokenKind::Newline;
        }
        let Some(byte) = self.cursor.peek() else {
            return TokenKind::Eof;
        };
        self.cursor.bump();
        match byte {
            b' ' | b'\t' => {
                self.cursor.eat_while(|b| b == b' ' || b == b'\t');
                TokenKind::Whitespace
            }
            b'#' => {
                self.comment();
                TokenKind::Comment
            }
            b'=' => TokenKind::Equals,
            b'.' => TokenKind::Dot,
            b',' => TokenKind::Comma,
            b'[' => TokenKind::LBracket,
            b']' => TokenKind::RBracket,
            b'{' => TokenKind::LBrace,
            b'}' => TokenKind::RBrace,
            b'"' => self.string(start, b'"'),
            b'\'' => self.string(start, b'\''),
            b if is_bare(b) => {
                self.cursor.eat_while(is_bare);
                TokenKind::Bare
            }
            b'\r' => {
                self.error(
                    "syntax/control-character",
                    "carriage return must be followed by a newline",
                    start,
                    start + 1,
                );
                TokenKind::Error
            }
            _ => {
                // Skip whole UTF-8 characters, so spans stay on char boundaries.
                self.cursor
                    .eat_while(|b| !b.is_ascii() || !is_token_start(b));
                let ch = self.source[start..].chars().next().unwrap_or_default();
                self.error(
                    "syntax/unexpected-character",
                    format!("unexpected character {ch:?}"),
                    start,
                    self.cursor.pos,
                );
                TokenKind::Error
            }
        }
    }

    fn comment(&mut self) {
        while let Some(byte) = self.cursor.peek() {
            if self.cursor.at_newline() {
                break;
            }
            if is_control(byte) {
                let pos = self.cursor.pos;
                self.error(
                    "syntax/control-character",
                    "control characters are not allowed in comments",
                    pos,
                    pos + 1,
                );
            }
            self.cursor.bump();
        }
    }

    /// Lexes the four string kinds. The opening `quote` is already consumed.
    fn string(&mut self, start: usize, quote: u8) -> TokenKind {
        let basic = quote == b'"';
        let multiline = self.cursor.peek() == Some(quote) && self.cursor.peek_at(1) == Some(quote);
        if multiline {
            self.cursor.bump();
            self.cursor.bump();
        }
        let kind = match (basic, multiline) {
            (true, false) => TokenKind::BasicString,
            (false, false) => TokenKind::LiteralString,
            (true, true) => TokenKind::MultilineBasicString,
            (false, true) => TokenKind::MultilineLiteralString,
        };

        loop {
            let Some(byte) = self.cursor.peek() else {
                self.unterminated(start);
                return kind;
            };
            if self.cursor.at_newline() {
                if !multiline {
                    self.unterminated(start);
                    return kind;
                }
                self.cursor.eat(b'\r');
                self.cursor.bump();
                continue;
            }
            if byte == quote {
                if !multiline {
                    self.cursor.bump();
                    return kind;
                }
                let run = self.quote_run(quote);
                if run >= 3 {
                    // Up to two quotes right before the closing `"""` belong to the content.
                    for _ in 0..run.min(5) {
                        self.cursor.bump();
                    }
                    return kind;
                }
                for _ in 0..run {
                    self.cursor.bump();
                }
                continue;
            }
            if byte == b'\\' && basic {
                self.escape(multiline);
                continue;
            }
            if is_control(byte) {
                let pos = self.cursor.pos;
                self.error(
                    "syntax/control-character",
                    "control characters are not allowed in strings",
                    pos,
                    pos + 1,
                );
            }
            self.cursor.bump();
        }
    }

    fn quote_run(&self, quote: u8) -> usize {
        self.cursor.bytes[self.cursor.pos..]
            .iter()
            .take_while(|&&b| b == quote)
            .count()
    }

    fn unterminated(&mut self, start: usize) {
        let end = self.cursor.pos;
        self.diagnostics.push(
            Diagnostic::new(
                "syntax/unterminated-string",
                Severity::Error,
                "unterminated string",
                span(start, end),
            )
            .with_help("add the closing quote"),
        );
    }

    /// Checks an escape sequence. The cursor is on the backslash. Invalid
    /// escapes only consume the backslash, so the string goes on normally.
    fn escape(&mut self, multiline: bool) {
        let start = self.cursor.pos;
        self.cursor.bump();
        match self.cursor.peek() {
            Some(b'b' | b't' | b'n' | b'f' | b'r' | b'"' | b'\\') => self.cursor.bump(),
            Some(b'u') => {
                self.cursor.bump();
                self.unicode_escape(start, 4);
            }
            Some(b'U') => {
                self.cursor.bump();
                self.unicode_escape(start, 8);
            }
            Some(b' ' | b'\t' | b'\n' | b'\r') if multiline => {
                // Line ending backslash: only whitespace may follow it on the line.
                let mut ahead = 0;
                while matches!(self.cursor.peek_at(ahead), Some(b' ' | b'\t')) {
                    ahead += 1;
                }
                let newline = self.cursor.peek_at(ahead) == Some(b'\n')
                    || (self.cursor.peek_at(ahead) == Some(b'\r')
                        && self.cursor.peek_at(ahead + 1) == Some(b'\n'));
                if !newline {
                    self.invalid_escape(start);
                }
            }
            _ => self.invalid_escape(start),
        }
    }

    fn unicode_escape(&mut self, start: usize, digits: usize) {
        let hex_start = self.cursor.pos;
        let mut count = 0;
        while count < digits && self.cursor.peek().is_some_and(|b| b.is_ascii_hexdigit()) {
            self.cursor.bump();
            count += 1;
        }
        let end = self.cursor.pos;
        if count < digits {
            self.error(
                "syntax/invalid-escape",
                format!("expected {digits} hex digits in unicode escape"),
                start,
                end,
            );
            return;
        }
        let valid = u32::from_str_radix(&self.source[hex_start..end], 16)
            .ok()
            .and_then(char::from_u32)
            .is_some();
        if !valid {
            self.error(
                "syntax/invalid-escape",
                "unicode escape is not a valid scalar value",
                start,
                end,
            );
        }
    }

    fn invalid_escape(&mut self, start: usize) {
        let len = self.source[start + 1..]
            .chars()
            .next()
            .map_or(0, char::len_utf8);
        let end = start + 1 + len;
        let text = &self.source[start..end];
        self.diagnostics.push(
            Diagnostic::new(
                "syntax/invalid-escape",
                Severity::Error,
                format!("invalid escape sequence {text:?}"),
                span(start, end),
            )
            .with_help(r#"valid escapes are \b \t \n \f \r \" \\ \uXXXX \UXXXXXXXX"#),
        );
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::lex;
    use crate::TokenKind::{self, *};

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source).0.into_iter().map(|token| token.kind).collect()
    }

    fn codes(source: &str) -> Vec<&'static str> {
        lex(source).1.into_iter().map(|d| d.code).collect()
    }

    fn assert_lossless(source: &str) {
        let (tokens, _) = lex(source);
        let mut offset = 0;
        let mut rebuilt = String::new();
        for token in &tokens {
            assert_eq!(token.span.start, offset, "gap or overlap in {source:?}");
            rebuilt.push_str(&source[token.span.range()]);
            offset = token.span.end;
        }
        assert_eq!(rebuilt, source);
        let last = tokens.last().expect("at least Eof");
        assert_eq!(last.kind, Eof);
        assert!(last.span.is_empty());
    }

    #[test]
    fn always_ends_with_eof() {
        assert_eq!(kinds(""), vec![Eof]);
    }

    #[test]
    fn key_value_with_comment() {
        assert_eq!(
            kinds("a.b = 1 # hi\r\n"),
            vec![
                Bare, Dot, Bare, Whitespace, Equals, Whitespace, Bare, Whitespace, Comment,
                Newline, Eof
            ]
        );
    }

    #[test]
    fn numbers_and_dates_split_on_dots_only() {
        assert_eq!(kinds("3.14"), vec![Bare, Dot, Bare, Eof]);
        assert_eq!(kinds("1979-05-27T07:32:00+01:00"), vec![Bare, Eof]);
        assert_eq!(kinds("-6.626e-34"), vec![Bare, Dot, Bare, Eof]);
    }

    #[test]
    fn punctuation() {
        assert_eq!(
            kinds("[[a]]{,}"),
            vec![
                LBracket, LBracket, Bare, RBracket, RBracket, LBrace, Comma, RBrace, Eof
            ]
        );
    }

    #[test]
    fn string_kinds() {
        assert_eq!(kinds(r#""a\"b""#), vec![BasicString, Eof]);
        assert_eq!(kinds("'a\\'"), vec![LiteralString, Eof]);
        assert_eq!(kinds("\"\""), vec![BasicString, Eof]);
        assert_eq!(kinds("''"), vec![LiteralString, Eof]);
        assert_eq!(kinds("\"\"\"a\n\"b\"\"\""), vec![MultilineBasicString, Eof]);
        assert_eq!(kinds("'''a\r\nb'''"), vec![MultilineLiteralString, Eof]);
        assert_eq!(codes(r#""é \U0001F600 \t""#), Vec::<&str>::new());
    }

    #[test]
    fn multiline_strings_take_up_to_two_extra_quotes() {
        let (tokens, diagnostics) = lex(r#""""a""""""#);
        assert_eq!(tokens[0].span.end, 9);
        assert_eq!(diagnostics, Vec::new());
        // Six quotes: the closing delimiter takes five, the sixth opens a new string.
        assert_eq!(
            kinds(r#""""a"""""""#)[..2],
            [MultilineBasicString, BasicString]
        );
    }

    #[test]
    fn line_ending_backslash() {
        assert_eq!(codes("\"\"\"a \\  \n  b\"\"\""), Vec::<&str>::new());
        assert_eq!(codes("\"\"\"a \\  b\"\"\""), vec!["syntax/invalid-escape"]);
        assert_eq!(
            codes("\"a \\\nb"),
            vec!["syntax/invalid-escape", "syntax/unterminated-string"]
        );
    }

    #[test]
    fn invalid_escapes() {
        assert_eq!(codes(r#""\x41""#), vec!["syntax/invalid-escape"]);
        assert_eq!(codes(r#""\u12""#), vec!["syntax/invalid-escape"]);
        assert_eq!(codes(r#""\uD800""#), vec!["syntax/invalid-escape"]);
        assert_eq!(codes(r#""\U00110000""#), vec!["syntax/invalid-escape"]);
        assert_eq!(codes("'\\x'"), Vec::<&str>::new());
    }

    #[test]
    fn unterminated_strings_stop_at_the_line_end() {
        assert_eq!(kinds("\"abc\nx"), vec![BasicString, Newline, Bare, Eof]);
        assert_eq!(codes("'abc"), vec!["syntax/unterminated-string"]);
        assert_eq!(codes("'''abc\n"), vec!["syntax/unterminated-string"]);
    }

    #[test]
    fn control_characters() {
        assert_eq!(codes("# a\u{7f}b"), vec!["syntax/control-character"]);
        assert_eq!(codes("# a\tb"), Vec::<&str>::new());
        assert_eq!(codes("\"a\u{0}\""), vec!["syntax/control-character"]);
        assert_eq!(kinds("a\rb"), vec![Bare, Error, Bare, Eof]);
        assert_eq!(codes("# a\rb\n"), vec!["syntax/control-character"]);
    }

    #[test]
    fn unexpected_characters_are_grouped() {
        assert_eq!(
            kinds("é€ = 1"),
            vec![Error, Whitespace, Equals, Whitespace, Bare, Eof]
        );
        assert_eq!(codes("é€"), vec!["syntax/unexpected-character"]);
    }

    fn toml_alphabet() -> impl Strategy<Value = String> {
        let chars: Vec<char> = "\"'#=[]{}.,\\ \t\r\nabeu0179:+-_é\u{0}\u{7f}"
            .chars()
            .collect();
        prop::collection::vec(prop::sample::select(chars), 0..64)
            .prop_map(|chars| chars.into_iter().collect())
    }

    proptest! {
        #[test]
        fn lossless_on_any_string(source in any::<String>()) {
            assert_lossless(&source);
        }

        #[test]
        fn lossless_on_random_bytes(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
            assert_lossless(&String::from_utf8_lossy(&bytes));
        }

        #[test]
        fn lossless_on_toml_like_input(source in toml_alphabet()) {
            assert_lossless(&source);
        }
    }
}
