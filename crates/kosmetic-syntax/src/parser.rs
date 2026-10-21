use kosmetic_diagnostics::{Diagnostic, Severity, Span};

use crate::literal::{classify, is_date};
use crate::{Element, Node, NodeKind, Token, TokenKind};

/// Builds the lossless tree from `tokens`, which must end with
/// [`TokenKind::Eof`]. Syntax errors are appended to `diagnostics`.
///
/// Recovery: after an error the parser skips to the end of the line, or to
/// the next `,` or closing bracket inside arrays and inline tables, wraps the
/// skipped tokens in a [`NodeKind::Error`] node and goes on.
pub(crate) fn parse(source: &str, tokens: &[Token], diagnostics: &mut Vec<Diagnostic>) -> Node {
    let mut parser = Parser {
        source,
        tokens,
        pos: 0,
        closers: Vec::new(),
        diagnostics,
    };
    parser.document()
}

/// Arrays and inline tables are parsed recursively: the limit keeps
/// malicious input from overflowing the stack.
const MAX_DEPTH: usize = 128;

struct Parser<'a> {
    source: &'a str,
    tokens: &'a [Token],
    pos: usize,
    /// Closing brackets of the arrays and inline tables being parsed.
    closers: Vec<TokenKind>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl Parser<'_> {
    fn current(&self) -> Token {
        self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek(&self) -> TokenKind {
        self.current().kind
    }

    fn nth(&self, n: usize) -> TokenKind {
        self.tokens
            .get(self.pos + n)
            .map_or(TokenKind::Eof, |token| token.kind)
    }

    fn text(&self, token: Token) -> &str {
        &self.source[token.span.range()]
    }

    fn bump(&mut self, out: &mut Vec<Element>) {
        out.push(Element::Token(self.current()));
        self.pos += 1;
    }

    fn eat_whitespace(&mut self, out: &mut Vec<Element>) {
        while self.peek() == TokenKind::Whitespace {
            self.bump(out);
        }
    }

    /// Kind of the next token that is not [`TokenKind::Whitespace`].
    fn peek_past_whitespace(&self) -> TokenKind {
        let mut n = 0;
        while self.nth(n) == TokenKind::Whitespace {
            n += 1;
        }
        self.nth(n)
    }

    fn finish(&self, kind: NodeKind, children: Vec<Element>) -> Node {
        let span = match (children.first(), children.last()) {
            (Some(first), Some(last)) => first.span().cover(last.span()),
            _ => Span::empty(self.current().span.start),
        };
        Node {
            kind,
            span,
            children,
        }
    }

    fn error(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::new(code, Severity::Error, message, span));
    }

    /// Reports an error on the current token, unless the lexer already
    /// reported it as an [`TokenKind::Error`] token.
    fn error_here(&mut self, code: &'static str, message: impl Into<String>) {
        if self.peek() != TokenKind::Error {
            let span = self.current().span;
            self.error(code, message, span);
        }
    }

    /// Skips tokens up to the end of the line, a comment, or a `,` or closing
    /// bracket of an enclosing array or inline table. Returns whether
    /// anything was skipped.
    fn recover(&mut self, out: &mut Vec<Element>) -> bool {
        let mut skipped = Vec::new();
        loop {
            let kind = self.peek();
            let stop = matches!(
                kind,
                TokenKind::Newline | TokenKind::Eof | TokenKind::Comment
            ) || (!self.closers.is_empty() && kind == TokenKind::Comma)
                || self.closers.contains(&kind);
            if stop {
                break;
            }
            self.bump(&mut skipped);
        }
        if skipped.is_empty() {
            return false;
        }
        out.push(Element::Node(self.finish(NodeKind::Error, skipped)));
        true
    }

    fn document(&mut self) -> Node {
        let mut root = Vec::new();
        let mut table: Option<(NodeKind, Vec<Element>)> = None;
        loop {
            let out = match &mut table {
                Some((_, children)) => children,
                None => &mut root,
            };
            match self.peek() {
                TokenKind::Eof => break,
                TokenKind::Whitespace | TokenKind::Newline | TokenKind::Comment => self.bump(out),
                TokenKind::LBracket => {
                    if let Some((kind, children)) = table.take() {
                        root.push(Element::Node(self.finish(kind, children)));
                    }
                    let (kind, header) = self.header();
                    let mut children = vec![Element::Node(header)];
                    self.line_end(&mut children);
                    table = Some((kind, children));
                }
                _ => {
                    let entry = self.key_value();
                    out.push(Element::Node(entry));
                    self.line_end(out);
                }
            }
        }
        if let Some((kind, children)) = table {
            root.push(Element::Node(self.finish(kind, children)));
        }
        self.bump(&mut root);
        self.finish(NodeKind::Document, root)
    }

    /// After a header or a key/value pair only whitespace and a comment may
    /// follow on the same line.
    fn line_end(&mut self, out: &mut Vec<Element>) {
        self.eat_whitespace(out);
        if self.peek() == TokenKind::Comment {
            self.bump(out);
        }
        if !matches!(self.peek(), TokenKind::Newline | TokenKind::Eof) {
            self.error_here("syntax/expected-newline", "expected a newline");
            self.recover(out);
        }
    }

    /// `[key]` or `[[key]]`. Returns the kind of the table it opens.
    fn header(&mut self) -> (NodeKind, Node) {
        let mut children = Vec::new();
        let open = self.current().span;
        self.bump(&mut children);
        let array = self.peek() == TokenKind::LBracket;
        if array {
            self.bump(&mut children);
        }
        self.eat_whitespace(&mut children);
        let key = self.key();
        children.push(Element::Node(key));
        self.eat_whitespace(&mut children);

        let closed = if array {
            self.peek() == TokenKind::RBracket && self.nth(1) == TokenKind::RBracket
        } else {
            self.peek() == TokenKind::RBracket
        };
        if closed {
            self.bump(&mut children);
            if array {
                self.bump(&mut children);
            }
        } else {
            let close = if array { "]]" } else { "]" };
            self.unclosed(
                &format!("expected `{close}` to close the table header"),
                open,
            );
            self.recover(&mut children);
        }

        if array {
            (
                NodeKind::ArrayTable,
                self.finish(NodeKind::ArrayTableHeader, children),
            )
        } else {
            (
                NodeKind::Table,
                self.finish(NodeKind::TableHeader, children),
            )
        }
    }

    fn unclosed(&mut self, message: &str, open: Span) {
        if self.peek() == TokenKind::Error {
            return;
        }
        let diagnostic = Diagnostic::new(
            "syntax/unclosed-delimiter",
            Severity::Error,
            message,
            self.current().span,
        )
        .with_label(open, "opened here");
        self.diagnostics.push(diagnostic);
    }

    fn key(&mut self) -> Node {
        let mut children = Vec::new();
        loop {
            match self.peek() {
                TokenKind::Bare => {
                    let token = self.current();
                    if self.text(token).contains(['+', ':']) {
                        self.error(
                            "syntax/invalid-key",
                            "bare keys may only contain letters, digits, `_` and `-`",
                            token.span,
                        );
                    }
                    self.bump(&mut children);
                }
                TokenKind::BasicString | TokenKind::LiteralString => self.bump(&mut children),
                TokenKind::MultilineBasicString | TokenKind::MultilineLiteralString => {
                    self.error_here("syntax/invalid-key", "multi-line strings cannot be keys");
                    self.bump(&mut children);
                }
                _ => {
                    self.error_here("syntax/expected-key", "expected a key");
                    break;
                }
            }
            if self.peek_past_whitespace() != TokenKind::Dot {
                break;
            }
            self.eat_whitespace(&mut children);
            self.bump(&mut children);
            self.eat_whitespace(&mut children);
        }
        self.finish(NodeKind::Key, children)
    }

    fn key_value(&mut self) -> Node {
        let mut children = Vec::new();
        let key = self.key();
        let has_key = !key.children.is_empty();
        children.push(Element::Node(key));
        self.eat_whitespace(&mut children);

        if self.peek() == TokenKind::Equals {
            self.bump(&mut children);
        } else {
            if has_key {
                self.error_here("syntax/expected-equals", "expected `=` after the key");
            }
            self.recover(&mut children);
            return self.finish(NodeKind::KeyValue, children);
        }

        self.eat_whitespace(&mut children);
        if let Some(value) = self.value() {
            children.push(Element::Node(value));
        } else {
            self.error_here("syntax/expected-value", "expected a value");
            self.recover(&mut children);
        }
        self.finish(NodeKind::KeyValue, children)
    }

    fn value(&mut self) -> Option<Node> {
        match self.peek() {
            TokenKind::BasicString
            | TokenKind::LiteralString
            | TokenKind::MultilineBasicString
            | TokenKind::MultilineLiteralString => {
                let mut children = Vec::new();
                self.bump(&mut children);
                Some(self.finish(NodeKind::String, children))
            }
            TokenKind::LBracket | TokenKind::LBrace if self.closers.len() >= MAX_DEPTH => {
                self.error_here(
                    "syntax/nesting-too-deep",
                    format!(
                        "arrays and inline tables cannot be nested more than {MAX_DEPTH} levels"
                    ),
                );
                let mut children = Vec::new();
                self.bump(&mut children);
                Some(self.finish(NodeKind::Error, children))
            }
            TokenKind::LBracket => Some(self.array()),
            TokenKind::LBrace => Some(self.inline_table()),
            TokenKind::Bare => Some(self.scalar()),
            _ => None,
        }
    }

    /// Numbers, booleans and date-times: the adjacent `Bare` and `Dot`
    /// tokens, plus the time after a date separated by a single space.
    fn scalar(&mut self) -> Node {
        let mut children = Vec::new();
        let start = self.current().span.start as usize;
        self.eat_scalar_tokens(&mut children);

        let date = &self.source[start..self.current().span.start as usize];
        let space = self.peek() == TokenKind::Whitespace && self.text(self.current()) == " ";
        let time_follows = self
            .tokens
            .get(self.pos + 1)
            .is_some_and(|next| next.kind == TokenKind::Bare && looks_like_time(self.text(*next)));
        if is_date(date) && space && time_follows {
            self.bump(&mut children);
            self.eat_scalar_tokens(&mut children);
        }

        let text = &self.source[start..self.current().span.start as usize];
        let kind = classify(text);
        if kind == NodeKind::Error {
            let message = format!("invalid value `{text}`");
            let node = self.finish(kind, children);
            self.error("syntax/invalid-value", message, node.span);
            return node;
        }
        self.finish(kind, children)
    }

    fn eat_scalar_tokens(&mut self, out: &mut Vec<Element>) {
        while matches!(self.peek(), TokenKind::Bare | TokenKind::Dot) {
            self.bump(out);
        }
    }

    fn array(&mut self) -> Node {
        let mut children = Vec::new();
        let open = self.current().span;
        self.bump(&mut children);
        self.closers.push(TokenKind::RBracket);
        let mut need_comma = false;
        let mut closed = false;
        loop {
            match self.peek() {
                TokenKind::Whitespace | TokenKind::Newline | TokenKind::Comment => {
                    self.bump(&mut children);
                }
                TokenKind::RBracket => {
                    self.bump(&mut children);
                    closed = true;
                    break;
                }
                TokenKind::Eof => break,
                TokenKind::Comma if need_comma => {
                    self.bump(&mut children);
                    need_comma = false;
                }
                _ if need_comma => {
                    self.error_here("syntax/unexpected-token", "expected `,` or `]`");
                    if !self.recover(&mut children) {
                        break;
                    }
                }
                _ => {
                    if let Some(value) = self.value() {
                        children.push(Element::Node(value));
                        need_comma = true;
                        continue;
                    }
                    self.error_here("syntax/expected-value", "expected a value");
                    if !self.recover(&mut children) {
                        if self.peek() != TokenKind::Comma {
                            break;
                        }
                        self.bump(&mut children);
                    }
                }
            }
        }
        self.closers.pop();
        if !closed {
            self.unclosed("expected `]` to close the array", open);
        }
        self.finish(NodeKind::Array, children)
    }

    fn inline_table(&mut self) -> Node {
        let mut children = Vec::new();
        let open = self.current().span;
        self.bump(&mut children);
        self.closers.push(TokenKind::RBrace);
        self.eat_whitespace(&mut children);
        let mut closed = self.peek() == TokenKind::RBrace;
        if closed {
            self.bump(&mut children);
        }
        while !closed {
            self.eat_whitespace(&mut children);
            let entry = self.key_value();
            children.push(Element::Node(entry));
            self.eat_whitespace(&mut children);
            match self.peek() {
                TokenKind::Comma => {
                    let comma = self.current().span;
                    self.bump(&mut children);
                    self.eat_whitespace(&mut children);
                    if self.peek() == TokenKind::RBrace {
                        self.error(
                            "syntax/trailing-comma",
                            "trailing commas are not allowed in inline tables",
                            comma,
                        );
                        self.bump(&mut children);
                        closed = true;
                    }
                }
                TokenKind::RBrace => {
                    self.bump(&mut children);
                    closed = true;
                }
                TokenKind::Newline | TokenKind::Eof | TokenKind::Comment => break,
                _ => {
                    self.error_here("syntax/unexpected-token", "expected `,` or `}`");
                    if !self.recover(&mut children) {
                        break;
                    }
                }
            }
        }
        self.closers.pop();
        if !closed {
            self.unclosed(
                "expected `}` to close the inline table, which must fit on one line",
                open,
            );
        }
        self.finish(NodeKind::InlineTable, children)
    }
}

/// `HH:` at the start of a token, so `1979-05-27 07:32:00` is one value.
fn looks_like_time(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() > 2 && bytes[0].is_ascii_digit() && bytes[1].is_ascii_digit() && bytes[2] == b':'
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use proptest::prelude::*;

    use crate::{Element, Node, Token, TokenKind, lex, parse};

    fn leaves(node: &Node, out: &mut Vec<Token>) {
        for child in &node.children {
            match child {
                Element::Node(node) => leaves(node, out),
                Element::Token(token) => out.push(*token),
            }
        }
    }

    /// Indented outline of the tree, trivia left out.
    fn dump(node: &Node, source: &str, depth: usize, out: &mut String) {
        let _ = writeln!(out, "{}{:?}", "  ".repeat(depth), node.kind);
        for child in &node.children {
            match child {
                Element::Node(node) => dump(node, source, depth + 1, out),
                Element::Token(token)
                    if !token.kind.is_trivia() && token.kind != TokenKind::Eof =>
                {
                    let text = &source[token.span.range()];
                    let _ = writeln!(out, "{}{:?} {text:?}", "  ".repeat(depth + 1), token.kind);
                }
                Element::Token(_) => {}
            }
        }
    }

    fn outline(source: &str) -> String {
        let mut out = String::new();
        dump(&parse(source).tree, source, 0, &mut out);
        out
    }

    fn codes(source: &str) -> Vec<&'static str> {
        parse(source)
            .diagnostics
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    fn assert_lossless(source: &str) {
        let parse = parse(source);
        let mut tokens = Vec::new();
        leaves(&parse.tree, &mut tokens);
        assert_eq!(
            tokens,
            lex(source).0,
            "tree leaves differ from tokens for {source:?}"
        );
        assert_eq!(parse.tree.span.end as usize, source.len());
    }

    #[test]
    fn builds_the_tree() {
        let source = "title = \"x\"  # name\n[server.http]\nports = [1, 2.5]\nwhen = 1979-05-27 07:32:00\n[[bin]]\nopt = { a = true }\n";
        let expected = r#"Document
  KeyValue
    Key
      Bare "title"
    Equals "="
    String
      BasicString "\"x\""
  Table
    TableHeader
      LBracket "["
      Key
        Bare "server"
        Dot "."
        Bare "http"
      RBracket "]"
    KeyValue
      Key
        Bare "ports"
      Equals "="
      Array
        LBracket "["
        Integer
          Bare "1"
        Comma ","
        Float
          Bare "2"
          Dot "."
          Bare "5"
        RBracket "]"
    KeyValue
      Key
        Bare "when"
      Equals "="
      DateTime
        Bare "1979-05-27"
        Bare "07:32:00"
  ArrayTable
    ArrayTableHeader
      LBracket "["
      LBracket "["
      Key
        Bare "bin"
      RBracket "]"
      RBracket "]"
    KeyValue
      Key
        Bare "opt"
      Equals "="
      InlineTable
        LBrace "{"
        KeyValue
          Key
            Bare "a"
          Equals "="
          Boolean
            Bare "true"
        RBrace "}"
"#;
        assert_eq!(outline(source), expected);
        assert_eq!(codes(source), Vec::<&str>::new());
        assert_lossless(source);
    }

    #[test]
    fn valid_documents_have_no_errors() {
        for source in [
            "",
            "a.b . c = 1\n\"q k\".'l' = 2",
            "a = [\n  1, # one\n  2,\n]\n",
            "a = []\nb = {}\nc = [[1], {x = 1}]",
            "1.2 = 3\ntrue = false",
            "[a]\n[[b.c]]\n[ d . e ]  # comment",
            "a = \"\"\"\nmulti\nline\"\"\"",
            "x = { a = [\n1,\n2] }",
        ] {
            assert_eq!(codes(source), Vec::<&str>::new(), "{source:?}");
            assert_lossless(source);
        }
    }

    #[test]
    fn reports_syntax_errors() {
        let cases = [
            ("a", vec!["syntax/expected-equals"]),
            ("a =", vec!["syntax/expected-value"]),
            ("= 1", vec!["syntax/expected-key"]),
            ("a = 1 2", vec!["syntax/expected-newline"]),
            ("a = 03", vec!["syntax/invalid-value"]),
            ("a+b = 1", vec!["syntax/invalid-key"]),
            ("[a", vec!["syntax/unclosed-delimiter"]),
            ("[[a]", vec!["syntax/unclosed-delimiter"]),
            ("[[a] ]", vec!["syntax/unclosed-delimiter"]),
            ("a = [1 2]", vec!["syntax/unexpected-token"]),
            ("a = [1,,2]", vec!["syntax/expected-value"]),
            ("a = [1", vec!["syntax/unclosed-delimiter"]),
            ("a = {b = 1,}", vec!["syntax/trailing-comma"]),
            (
                "a = {b = 1\n}",
                vec!["syntax/unclosed-delimiter", "syntax/expected-key"],
            ),
            ("a = 1979-05-27  07:32:00", vec!["syntax/expected-newline"]),
            ("'''k''' = 1", vec!["syntax/invalid-key"]),
        ];
        for (source, expected) in cases {
            assert_eq!(codes(source), expected, "{source:?}");
            assert_lossless(source);
        }
    }

    #[test]
    fn recovers_at_the_end_of_the_line() {
        let source = "a = \nb = ]\n[t\nc = 1\n";
        assert_eq!(
            codes(source),
            vec![
                "syntax/expected-value",
                "syntax/expected-value",
                "syntax/unclosed-delimiter"
            ]
        );
        assert!(outline(source).contains("Bare \"c\""));
    }

    #[test]
    fn deep_nesting_does_not_overflow_the_stack() {
        let source = format!("a = {}", "[{a=".repeat(100_000));
        assert!(codes(&source).contains(&"syntax/nesting-too-deep"));
        assert_lossless(&source);
    }

    #[test]
    fn lexer_errors_are_not_reported_twice() {
        assert_eq!(codes("é = 1"), vec!["syntax/unexpected-character"]);
    }

    fn toml_alphabet() -> impl Strategy<Value = String> {
        let pieces = prop::sample::select(vec![
            "a",
            "b.c",
            "\"s\"",
            "'l'",
            "1",
            "2.5",
            "true",
            "1979-05-27",
            " 07:32:00",
            " ",
            "\n",
            "=",
            ",",
            "[",
            "]",
            "[[",
            "]]",
            "{",
            "}",
            "#c",
            "\"\"\"",
            "é",
            "\r",
        ]);
        prop::collection::vec(pieces, 0..40).prop_map(|pieces| pieces.concat())
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
