//! Second formatting stage: blank lines and the end of the file.

use std::collections::HashSet;

use kosmetic_syntax::{NodeKind, TokenKind, parse};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Blank,
    Comment,
    Header,
    Other,
}

struct Line {
    start: usize,
    end: usize,
    kind: Kind,
}

/// Splits the source into lines at the line breaks that are tokens, so a
/// multi-line string is part of one line.
fn split(source: &str) -> (Vec<Line>, &str) {
    let parse = parse(source);
    let headers: HashSet<u32> = parse
        .tree
        .descendants()
        .filter(|node| {
            matches!(
                node.kind,
                NodeKind::TableHeader | NodeKind::ArrayTableHeader
            )
        })
        .map(|node| node.span.start)
        .collect();
    let eol = parse
        .tokens
        .iter()
        .find(|token| token.kind == TokenKind::Newline)
        .map_or("\n", |token| &source[token.span.range()]);

    let mut lines = Vec::new();
    let mut start = 0;
    let mut first = None;
    for token in &parse.tokens {
        match token.kind {
            TokenKind::Whitespace => {}
            TokenKind::Newline | TokenKind::Eof => {
                let kind = match first {
                    None => Kind::Blank,
                    Some((TokenKind::Comment, _)) => Kind::Comment,
                    Some((_, at)) if headers.contains(&at) => Kind::Header,
                    Some(_) => Kind::Other,
                };
                lines.push(Line {
                    start,
                    end: token.span.start as usize,
                    kind,
                });
                start = token.span.end as usize;
                first = None;
            }
            kind => {
                first.get_or_insert((kind, token.span.start));
            }
        }
    }
    (lines, eol)
}

/// Rebuilds a valid document with at most one blank line in a row, one blank
/// line before every table header (or before the comments right above it), no
/// blank lines at the start and the end, and exactly one final newline.
pub(crate) fn arrange(source: &str) -> String {
    let (lines, eol) = split(source);
    let mut out = String::with_capacity(source.len());
    let mut blanks = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.kind == Kind::Blank {
            blanks += 1;
            continue;
        }
        let comments_above_header = line.kind == Kind::Comment
            && (i == 0 || lines[i - 1].kind != Kind::Comment)
            && lines[i..]
                .iter()
                .find(|other| other.kind != Kind::Comment)
                .is_some_and(|other| other.kind == Kind::Header);
        let starts_section = comments_above_header
            || (line.kind == Kind::Header && (i == 0 || lines[i - 1].kind != Kind::Comment));
        let wanted = if out.is_empty() {
            0
        } else {
            usize::from(starts_section || blanks > 0)
        };
        out.push_str(&eol.repeat(wanted));
        out.push_str(&source[line.start..line.end]);
        out.push_str(eol);
        blanks = 0;
    }
    out
}
