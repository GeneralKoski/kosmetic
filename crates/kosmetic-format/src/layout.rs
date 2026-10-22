//! First formatting stage: the whitespace inside lines, the indentation of
//! multi-line arrays, line endings and trailing commas.
//!
//! The tree is flattened into its leaves. For every two neighbouring tokens
//! that are not whitespace, the innermost node that contains both decides how
//! much space belongs between them.

use kosmetic_diagnostics::{Fix, Span, apply_fixes};
use kosmetic_syntax::{Element, Node, NodeKind, Token, TokenKind, parse};

const INDENT: &str = "  ";

struct NodeInfo {
    kind: NodeKind,
    parent: Option<usize>,
    depth: usize,
}

struct Leaf {
    token: Token,
    parent: usize,
}

struct Layout<'a> {
    source: &'a str,
    nodes: Vec<NodeInfo>,
    leaves: Vec<Leaf>,
    fixes: Vec<Fix>,
}

/// Applies the in-line layout to a valid document.
pub(crate) fn layout(source: &str) -> String {
    let parse = parse(source);
    let mut layout = Layout {
        source,
        nodes: Vec::new(),
        leaves: Vec::new(),
        fixes: Vec::new(),
    };
    layout.flatten(&parse.tree, None);
    layout.space_tokens();
    apply_fixes(source, &layout.fixes).0
}

impl Layout<'_> {
    fn flatten(&mut self, node: &Node, parent: Option<usize>) {
        let id = self.nodes.len();
        let depth = parent.map_or(0, |parent| self.nodes[parent].depth + 1);
        self.nodes.push(NodeInfo {
            kind: node.kind,
            parent,
            depth,
        });
        if node.kind == NodeKind::Array {
            self.trailing_comma(node);
        }
        for child in &node.children {
            match child {
                Element::Node(child) => self.flatten(child, Some(id)),
                Element::Token(token) => self.leaves.push(Leaf {
                    token: *token,
                    parent: id,
                }),
            }
        }
    }

    /// A multi-line array whose last item is followed by a line break gets a
    /// comma after the item, so adding a line later changes one line only.
    fn trailing_comma(&mut self, array: &Node) {
        let Some((Element::Token(close), inner)) = array.children.split_last() else {
            return;
        };
        if close.kind != TokenKind::RBracket {
            return;
        }
        let is_trivia =
            |element: &Element| matches!(element, Element::Token(token) if token.kind.is_trivia());
        let Some(last) = inner.iter().rposition(|element| !is_trivia(element)) else {
            return;
        };
        let line_break = inner[last + 1..]
            .iter()
            .any(|element| matches!(element, Element::Token(t) if t.kind == TokenKind::Newline));
        if matches!(inner[last], Element::Node(_)) && line_break {
            let at = inner[last].span().end;
            self.fixes.push(Fix {
                span: Span::empty(at),
                replacement: ",".to_owned(),
            });
        }
    }

    /// Innermost node that contains both leaves.
    fn common_node(&self, a: usize, b: usize) -> NodeKind {
        let (mut a, mut b) = (self.leaves[a].parent, self.leaves[b].parent);
        while a != b {
            if self.nodes[a].depth >= self.nodes[b].depth {
                a = self.nodes[a].parent.expect("the root contains every leaf");
            } else {
                b = self.nodes[b].parent.expect("the root contains every leaf");
            }
        }
        self.nodes[a].kind
    }

    /// Indentation of a line that starts with the given leaf: two spaces for
    /// every array or inline table that is open at that point.
    fn indentation(&self, leaf: usize) -> String {
        let mut depth = 0;
        let mut node = Some(self.leaves[leaf].parent);
        while let Some(id) = node {
            depth += usize::from(matches!(
                self.nodes[id].kind,
                NodeKind::Array | NodeKind::InlineTable
            ));
            node = self.nodes[id].parent;
        }
        // The brackets of an array belong to it but are not inside it.
        let is_bracket = matches!(
            self.leaves[leaf].token.kind,
            TokenKind::LBracket | TokenKind::RBracket | TokenKind::LBrace | TokenKind::RBrace
        ) && matches!(
            self.nodes[self.leaves[leaf].parent].kind,
            NodeKind::Array | NodeKind::InlineTable
        );
        INDENT.repeat(depth - usize::from(is_bracket))
    }

    fn space_tokens(&mut self) {
        let eol = self
            .leaves
            .iter()
            .find(|leaf| leaf.token.kind == TokenKind::Newline)
            .map_or("\n", |leaf| &self.source[leaf.token.span.range()]);
        let mut previous: Option<usize> = None;
        for next in 0..self.leaves.len() {
            let token = self.leaves[next].token;
            match token.kind {
                TokenKind::Whitespace => continue,
                TokenKind::Newline if &self.source[token.span.range()] != eol => {
                    self.fixes.push(Fix {
                        span: token.span,
                        replacement: eol.to_owned(),
                    });
                }
                TokenKind::Comment => {
                    let text = &self.source[token.span.range()];
                    let kept = text.trim_end_matches([' ', '\t']).len();
                    if kept < text.len() {
                        self.fixes.push(Fix {
                            span: Span::new(
                                token.span.start + u32::try_from(kept).unwrap_or(0),
                                token.span.end,
                            ),
                            replacement: String::new(),
                        });
                    }
                }
                _ => {}
            }

            let gap = next
                .checked_sub(1)
                .map(|i| self.leaves[i].token)
                .filter(|leaf| leaf.kind == TokenKind::Whitespace);
            if let Some(wanted) = self.wanted_space(previous, next, gap.is_some()) {
                let (span, text) = match (gap, previous) {
                    (Some(gap), _) => (gap.span, &self.source[gap.span.range()]),
                    (None, Some(p)) => (Span::empty(self.leaves[p].token.span.end), ""),
                    (None, None) => (Span::empty(0), ""),
                };
                if text != wanted {
                    self.fixes.push(Fix {
                        span,
                        replacement: wanted,
                    });
                }
            }
            previous = Some(next);
        }
    }

    /// The space that belongs between two tokens, or `None` to leave it alone.
    fn wanted_space(&self, previous: Option<usize>, next: usize, has_gap: bool) -> Option<String> {
        let next_kind = self.leaves[next].token.kind;
        if matches!(next_kind, TokenKind::Newline | TokenKind::Eof) {
            return Some(String::new());
        }
        let Some(previous) = previous else {
            return Some(self.indentation(next));
        };
        let previous_kind = self.leaves[previous].token.kind;
        if previous_kind == TokenKind::Newline {
            return Some(self.indentation(next));
        }
        if next_kind == TokenKind::Comment {
            return (!has_gap).then(|| " ".to_owned());
        }
        let either = |kind| previous_kind == kind || next_kind == kind;
        if either(TokenKind::Dot) {
            return Some(String::new());
        }
        if either(TokenKind::Equals) {
            return Some(" ".to_owned());
        }
        let space = |wanted: &str| Some(wanted.to_owned());
        match self.common_node(previous, next) {
            NodeKind::TableHeader | NodeKind::ArrayTableHeader
                if previous_kind == TokenKind::LBracket || next_kind == TokenKind::RBracket =>
            {
                space("")
            }
            NodeKind::Array => match (previous_kind, next_kind) {
                (TokenKind::LBracket, _) | (_, TokenKind::RBracket | TokenKind::Comma) => space(""),
                (TokenKind::Comma, _) => space(" "),
                _ => None,
            },
            NodeKind::InlineTable => match (previous_kind, next_kind) {
                (TokenKind::LBrace, TokenKind::RBrace) | (_, TokenKind::Comma) => space(""),
                (TokenKind::LBrace | TokenKind::Comma, _) | (_, TokenKind::RBrace) => space(" "),
                _ => None,
            },
            _ => None,
        }
    }
}
