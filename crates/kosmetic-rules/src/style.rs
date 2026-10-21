//! Style rules: they never change what the file means, only how it reads.

use kosmetic_diagnostics::{Fix, Label, Span};
use kosmetic_syntax::{NodeKind, TokenKind, string_value};

use crate::{Level, Rule, RuleContext, RuleMeta};

/// Length of a piece of the source as an offset. Fits in `u32`: the lexer
/// rejects sources over 4 GiB.
#[allow(clippy::cast_possible_truncation)]
fn len(text: &str) -> u32 {
    text.len() as u32
}

fn fix(span: Span, replacement: &str) -> Fix {
    Fix {
        span,
        replacement: replacement.to_owned(),
    }
}

/// Flags spaces and tabs at the end of a line. Works on tokens, so trailing
/// spaces inside multi-line strings, which are part of the value, are left
/// alone. It is the reference example for writing new rules.
pub struct NoTrailingWhitespace;

static NO_TRAILING_WHITESPACE: RuleMeta = RuleMeta {
    code: "style/no-trailing-whitespace",
    description: "Disallow spaces and tabs at the end of a line",
    default_level: Level::Warn,
};

impl Rule for NoTrailingWhitespace {
    fn meta(&self) -> &'static RuleMeta {
        &NO_TRAILING_WHITESPACE
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        let tokens = &ctx.parse.tokens;
        for (i, token) in tokens.iter().enumerate() {
            let span = match token.kind {
                TokenKind::Whitespace
                    if matches!(
                        tokens.get(i + 1).map(|next| next.kind),
                        Some(TokenKind::Newline | TokenKind::Eof)
                    ) =>
                {
                    token.span
                }
                TokenKind::Comment => {
                    let text = &ctx.source[token.span.range()];
                    let kept = len(text.trim_end_matches([' ', '\t']));
                    if kept == token.span.len() {
                        continue;
                    }
                    Span::new(token.span.start + kept, token.span.end)
                }
                _ => continue,
            };
            let diagnostic = ctx.report("trailing whitespace", span);
            diagnostic.help = Some("remove the spaces at the end of the line".to_owned());
            diagnostic.fix = Some(fix(span, ""));
        }
    }
}

/// Requires the file to end with exactly one newline.
pub struct FinalNewline;

static FINAL_NEWLINE: RuleMeta = RuleMeta {
    code: "style/final-newline",
    description: "Require exactly one newline at the end of the file",
    default_level: Level::Warn,
};

impl Rule for FinalNewline {
    fn meta(&self) -> &'static RuleMeta {
        &FINAL_NEWLINE
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        let source = ctx.source;
        let content = len(source.trim_end_matches(['\n', '\r']));
        let end = len(source);
        if content == 0 {
            return;
        }
        if content == end {
            ctx.report("missing newline at the end of the file", Span::empty(end))
                .fix = Some(fix(Span::empty(end), "\n"));
            return;
        }
        let first = if source[content as usize..].starts_with("\r\n") {
            2
        } else {
            1
        };
        if content + first < end {
            let span = Span::new(content + first, end);
            ctx.report("blank lines at the end of the file", span).fix = Some(fix(span, ""));
        }
    }
}

/// Allows at most one blank line in a row. Blank lines at the end of the
/// file are left to [`FinalNewline`], those inside strings are content.
pub struct NoMultipleBlankLines;

static NO_MULTIPLE_BLANK_LINES: RuleMeta = RuleMeta {
    code: "style/no-multiple-blank-lines",
    description: "Disallow more than one blank line in a row",
    default_level: Level::Warn,
};

impl Rule for NoMultipleBlankLines {
    fn meta(&self) -> &'static RuleMeta {
        &NO_MULTIPLE_BLANK_LINES
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        let tokens = &ctx.parse.tokens;
        let mut i = 0;
        while i < tokens.len() {
            if tokens[i].kind != TokenKind::Newline {
                i += 1;
                continue;
            }
            let at_start = tokens[..i].iter().all(|t| t.kind == TokenKind::Whitespace);
            let mut newlines = vec![i];
            let mut j = i + 1;
            while let Some(token) = tokens.get(j) {
                match token.kind {
                    TokenKind::Newline => newlines.push(j),
                    TokenKind::Whitespace => {}
                    _ => break,
                }
                j += 1;
            }
            i = j;
            if tokens
                .get(j)
                .is_none_or(|token| token.kind == TokenKind::Eof)
            {
                continue;
            }
            // The newline that ends the one blank line we keep.
            let keep = usize::from(!at_start);
            if newlines.len() > keep + 1 {
                let last = newlines[newlines.len() - 1];
                let span = Span::new(tokens[newlines[keep]].span.end, tokens[last].span.end);
                ctx.report("more than one blank line", span).fix = Some(fix(span, ""));
            }
        }
    }
}

/// Requires every line ending to match the first one, `\n` or `\r\n`.
pub struct ConsistentLineEndings;

static CONSISTENT_LINE_ENDINGS: RuleMeta = RuleMeta {
    code: "style/consistent-line-endings",
    description: "Require the same line ending, LF or CRLF, on every line",
    default_level: Level::Warn,
};

impl Rule for ConsistentLineEndings {
    fn meta(&self) -> &'static RuleMeta {
        &CONSISTENT_LINE_ENDINGS
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        let name = |ending: &str| if ending == "\n" { "LF" } else { "CRLF" };
        let mut first = None;
        for token in ctx
            .parse
            .tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Newline)
        {
            let ending = &ctx.source[token.span.range()];
            let Some(expected) = first else {
                first = Some(ending);
                continue;
            };
            if ending != expected {
                let message = format!(
                    "{} line ending in a file that uses {}",
                    name(ending),
                    name(expected)
                );
                ctx.report(message, token.span).fix = Some(fix(token.span, expected));
            }
        }
    }
}

/// Requires `key = value`, with exactly one space on each side of `=`.
pub struct SpacesAroundEquals;

static SPACES_AROUND_EQUALS: RuleMeta = RuleMeta {
    code: "style/spaces-around-equals",
    description: "Require exactly one space on each side of `=`",
    default_level: Level::Warn,
};

impl Rule for SpacesAroundEquals {
    fn meta(&self) -> &'static RuleMeta {
        &SPACES_AROUND_EQUALS
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        for entry in ctx.parse.tree.descendants() {
            if entry.kind != NodeKind::KeyValue {
                continue;
            }
            let key = entry.child(NodeKind::Key);
            let value = entry.nodes().find(|node| node.kind.is_value());
            let (Some(key), Some(value)) = (key, value) else {
                continue;
            };
            let span = Span::new(key.span.end, value.span.start);
            if &ctx.source[span.range()] != " = " {
                ctx.report("expected one space on each side of `=`", span)
                    .fix = Some(fix(span, " = "));
            }
        }
    }
}

/// Requires a space between `#` and the comment text. Commented-out TOML
/// (`#key = 1`, `#[table]`) is left alone.
pub struct CommentSpace;

static COMMENT_SPACE: RuleMeta = RuleMeta {
    code: "style/comment-space",
    description: "Require a space after `#` in comments",
    default_level: Level::Warn,
};

impl Rule for CommentSpace {
    fn meta(&self) -> &'static RuleMeta {
        &COMMENT_SPACE
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        for token in ctx
            .parse
            .tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Comment)
        {
            let text = &ctx.source[token.span.range()][1..];
            // `##` banners, `#:` and `#!` markers and commented-out TOML are fine.
            let commented_out = text.starts_with('[') || text.contains('=');
            if text.is_empty() || text.starts_with([' ', '\t', '#', ':', '!']) || commented_out {
                continue;
            }
            let hash = Span::new(token.span.start, token.span.start + 1);
            ctx.report("missing space after `#`", hash).fix = Some(fix(Span::empty(hash.end), " "));
        }
    }
}

/// Flags quoted keys that could be bare, like `"name" = 1`.
pub struct PreferBareKeys;

static PREFER_BARE_KEYS: RuleMeta = RuleMeta {
    code: "style/prefer-bare-keys",
    description: "Disallow quotes around keys that do not need them",
    default_level: Level::Warn,
};

impl Rule for PreferBareKeys {
    fn meta(&self) -> &'static RuleMeta {
        &PREFER_BARE_KEYS
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        for key in ctx.parse.tree.descendants() {
            if key.kind != NodeKind::Key {
                continue;
            }
            for token in key.tokens() {
                if !matches!(
                    token.kind,
                    TokenKind::BasicString | TokenKind::LiteralString
                ) {
                    continue;
                }
                let text = &ctx.source[token.span.range()];
                let Some(inner) = text.get(1..text.len().saturating_sub(1)) else {
                    continue;
                };
                let terminated = text.len() >= 2 && text.ends_with(&text[..1]);
                let bare = !inner.is_empty()
                    && inner
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
                if terminated && bare {
                    ctx.report(
                        format!("quotes are not needed around `{inner}`"),
                        token.span,
                    )
                    .fix = Some(fix(token.span, inner));
                }
            }
        }
    }
}

/// Requires the keys of each table to be in alphabetical order. Off by
/// default: useful for `[dependencies]`, noisy elsewhere.
pub struct SortedKeys;

static SORTED_KEYS: RuleMeta = RuleMeta {
    code: "style/sorted-keys",
    description: "Require the keys of each table to be sorted alphabetically",
    default_level: Level::Off,
};

impl Rule for SortedKeys {
    fn meta(&self) -> &'static RuleMeta {
        &SORTED_KEYS
    }

    fn check(&self, ctx: &mut RuleContext<'_>) {
        let source = ctx.source;
        let segments = |key: &kosmetic_syntax::Node| -> Vec<String> {
            key.tokens()
                .filter(|t| !matches!(t.kind, TokenKind::Dot | TokenKind::Whitespace))
                .map(|t| string_value(t.kind, &source[t.span.range()]))
                .collect()
        };
        for container in ctx.parse.tree.descendants() {
            if !matches!(
                container.kind,
                NodeKind::Document | NodeKind::Table | NodeKind::ArrayTable | NodeKind::InlineTable
            ) {
                continue;
            }
            let keys: Vec<_> = container
                .nodes()
                .filter(|node| node.kind == NodeKind::KeyValue)
                .filter_map(|entry| entry.child(NodeKind::Key))
                .map(|key| (segments(key), key.span))
                .collect();
            for pair in keys.windows(2) {
                let ((previous, previous_span), (key, span)) = (&pair[0], &pair[1]);
                if key < previous {
                    let message = format!(
                        "key `{}` should come before `{}`",
                        key.join("."),
                        previous.join(".")
                    );
                    ctx.report(message, *span).labels.push(Label {
                        span: *previous_span,
                        message: "this key comes after it".to_owned(),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use kosmetic_diagnostics::Span;

    use crate::Level;
    use crate::testing::{fixed, lint};

    fn spans(code: &str, source: &str) -> Vec<Span> {
        lint(code, source, None)
            .into_iter()
            .map(|d| d.span)
            .collect()
    }

    fn assert_off(code: &str, source: &str) {
        assert_ne!(lint(code, source, None), Vec::new(), "{code} should fire");
        assert_eq!(lint(code, source, Some(Level::Off)), Vec::new());
    }

    #[test]
    fn trailing_whitespace() {
        let code = "style/no-trailing-whitespace";
        assert_eq!(
            spans(code, "a = 1  \nb = 2\t\n# c  \n"),
            vec![Span::new(5, 7), Span::new(13, 14), Span::new(18, 20)]
        );
        assert_eq!(spans(code, "a = 1\r\nb = \"\"\"x  \ny\"\"\"\n"), Vec::new());
        assert_eq!(fixed(code, "a = 1 \n"), "a = 1\n");
        assert_off(code, "a = 1  \n");
    }

    #[test]
    fn final_newline() {
        let code = "style/final-newline";
        assert_eq!(fixed(code, "a = 1"), "a = 1\n");
        assert_eq!(fixed(code, "a = 1\n\n\n"), "a = 1\n");
        assert_eq!(fixed(code, "a = 1\r\n\r\n"), "a = 1\r\n");
        assert_eq!(spans(code, "a = 1\n"), Vec::new());
        assert_eq!(spans(code, ""), Vec::new());
        assert_off(code, "a = 1");
    }

    #[test]
    fn multiple_blank_lines() {
        let code = "style/no-multiple-blank-lines";
        assert_eq!(fixed(code, "a = 1\n\n\n  \nb = 2\n"), "a = 1\n\nb = 2\n");
        assert_eq!(fixed(code, "\n\n\na = 1\n"), "\na = 1\n");
        assert_eq!(
            spans(code, "a = 1\n\nb = 2\n# c\n\nd = \"\"\"\n\n\n\"\"\"\n"),
            Vec::new()
        );
        assert_eq!(spans(code, "a = 1\n\n\n\n"), Vec::new());
        assert_off(code, "a = 1\n\n\nb = 2\n");
    }

    #[test]
    fn line_endings() {
        let code = "style/consistent-line-endings";
        assert_eq!(
            fixed(code, "a = 1\nb = 2\r\nc = 3\r\n"),
            "a = 1\nb = 2\nc = 3\n"
        );
        assert_eq!(fixed(code, "a = 1\r\nb = 2\n"), "a = 1\r\nb = 2\r\n");
        assert_eq!(spans(code, "a = 1\r\nb = 2\r\n"), Vec::new());
        assert_off(code, "a = 1\nb = 2\r\n");
    }

    #[test]
    fn spaces_around_equals() {
        let code = "style/spaces-around-equals";
        assert_eq!(
            fixed(code, "a=1\nb  =  2\nc = {d=3}\n"),
            "a = 1\nb = 2\nc = {d = 3}\n"
        );
        assert_eq!(spans(code, "a = 1\nb.c = { d = [1] }\n"), Vec::new());
        assert_eq!(spans(code, "a =\n"), Vec::new());
        assert_off(code, "a=1\n");
    }

    #[test]
    fn comment_space() {
        let code = "style/comment-space";
        assert_eq!(fixed(code, "#a\nb = 1 #c\n"), "# a\nb = 1 # c\n");
        assert_eq!(spans(code, "# a\n#\n## banner ##\n#\tx\n"), Vec::new());
        assert_eq!(
            spans(code, "#key = 1\n#[table]\nv = 1 #:version\n"),
            Vec::new()
        );
        assert_off(code, "#a\n");
    }

    #[test]
    fn prefer_bare_keys() {
        let code = "style/prefer-bare-keys";
        assert_eq!(
            fixed(code, "\"a\" = 1\n'b-c'.\"d_1\" = 2\n[\"t\"]\n"),
            "a = 1\nb-c.d_1 = 2\n[t]\n"
        );
        assert_eq!(
            spans(code, "\"a b\" = 1\n\"\" = 2\n\"a.b\" = 3\n'é' = 4\n"),
            Vec::new()
        );
        assert_off(code, "\"a\" = 1\n");
    }

    #[test]
    fn sorted_keys() {
        let code = "style/sorted-keys";
        let source = "b = 1\na = 2\n[t]\nx.y = 1\nx.a = 2\n[u]\nz = { b = 1, a = 2 }\n";
        let diagnostics = lint(code, source, Some(Level::Warn));
        let messages: Vec<_> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "key `a` should come before `b`",
                "key `x.a` should come before `x.y`",
                "key `a` should come before `b`",
            ]
        );
        assert_eq!(diagnostics[0].labels[0].span, Span::new(0, 1));
        assert_eq!(
            lint(code, "a = 1\n\"b\" = 2\n[t]\nc = 3\n", Some(Level::Warn)),
            Vec::new()
        );
        assert_eq!(lint(code, source, None), Vec::new(), "off by default");
    }
}
