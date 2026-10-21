use kosmetic_diagnostics::Span;

/// Kind of a token.
///
/// TOML is context sensitive: `true`, `1979-05-27` and `inf` can all be bare
/// keys on the left of `=` and values on the right. The lexer therefore emits
/// a single [`TokenKind::Bare`] for those, and the parser decides what they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// Spaces and tabs.
    Whitespace,
    /// `\n` or `\r\n`.
    Newline,
    /// `# ...` up to the end of the line, line ending excluded.
    Comment,

    /// `=`
    Equals,
    /// `.`
    Dot,
    /// `,`
    Comma,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `{`
    LBrace,
    /// `}`
    RBrace,

    /// `"..."`
    BasicString,
    /// `'...'`
    LiteralString,
    /// `"""..."""`
    MultilineBasicString,
    /// `'''...'''`
    MultilineLiteralString,

    /// Bare key, boolean, number or date-time. Disambiguated by the parser.
    Bare,

    /// A byte sequence the lexer could not make sense of. Always comes with a diagnostic.
    Error,
    /// End of file. Always the last token, with an empty span.
    Eof,
}

impl TokenKind {
    /// Whitespace, newlines and comments, which carry no meaning for the grammar.
    pub fn is_trivia(self) -> bool {
        matches!(self, Self::Whitespace | Self::Newline | Self::Comment)
    }
}

/// A token and its position in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    /// What the token is.
    pub kind: TokenKind,
    /// Where it is in the source.
    pub span: Span,
}
