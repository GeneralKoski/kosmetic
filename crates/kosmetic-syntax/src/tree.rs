use kosmetic_diagnostics::Span;

use crate::Token;

/// Kind of a node in the lossless tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// The whole file. Ends with the [`crate::TokenKind::Eof`] token.
    Document,
    /// A `[table]` header and the entries that follow it.
    Table,
    /// `[` key `]`
    TableHeader,
    /// A `[[array.of.tables]]` header and the entries that follow it.
    ArrayTable,
    /// `[[` key `]]`
    ArrayTableHeader,
    /// key `=` value
    KeyValue,
    /// One or more segments separated by dots: `a`, `"b c".d`.
    Key,
    /// One of the four string tokens.
    String,
    /// `42`, `0xff`, `1_000`
    Integer,
    /// `3.14`, `1e6`, `inf`, `nan`. Spans several tokens, since `.` is a separate token.
    Float,
    /// `true` or `false`
    Boolean,
    /// Offset or local date-time, local date or local time.
    DateTime,
    /// `[` values `]`
    Array,
    /// `{` key/value pairs `}`
    InlineTable,
    /// Tokens skipped while recovering from a syntax error, or a malformed value.
    Error,
}

impl NodeKind {
    /// Whether the node can be on the right of `=`.
    pub fn is_value(self) -> bool {
        matches!(
            self,
            Self::String
                | Self::Integer
                | Self::Float
                | Self::Boolean
                | Self::DateTime
                | Self::Array
                | Self::InlineTable
        )
    }
}

/// A node of the lossless tree. Its leaves, in order, are every token of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// What the node is.
    pub kind: NodeKind,
    /// Range covered by the node's tokens.
    pub span: Span,
    /// Child nodes and tokens, trivia included, in source order.
    pub children: Vec<Element>,
}

/// A child of a [`Node`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Element {
    /// An inner node.
    Node(Node),
    /// A leaf.
    Token(Token),
}

impl Element {
    /// Range covered by the element.
    pub fn span(&self) -> Span {
        match self {
            Self::Node(node) => node.span,
            Self::Token(token) => token.span,
        }
    }
}

impl Node {
    /// Direct child nodes.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().filter_map(|element| match element {
            Element::Node(node) => Some(node),
            Element::Token(_) => None,
        })
    }

    /// Direct child tokens.
    pub fn tokens(&self) -> impl Iterator<Item = &Token> {
        self.children.iter().filter_map(|element| match element {
            Element::Token(token) => Some(token),
            Element::Node(_) => None,
        })
    }

    /// First direct child node of the given kind.
    pub fn child(&self, kind: NodeKind) -> Option<&Node> {
        self.nodes().find(|node| node.kind == kind)
    }

    /// Source text covered by the node.
    pub fn text<'s>(&self, source: &'s str) -> &'s str {
        &source[self.span.range()]
    }
}
