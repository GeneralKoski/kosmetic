//! Style rules: they never change what the file means, only how it reads.

use kosmetic_diagnostics::{Fix, Span};

use crate::{Level, Rule, RuleContext, RuleMeta};

/// Flags spaces and tabs at the end of a line.
///
/// Works on the raw text, so it already runs before the parser exists. It is
/// the reference example for writing new rules.
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
        let mut line_start = 0usize;
        for line in ctx.source.split_inclusive('\n') {
            let content = line.trim_end_matches(['\n', '\r']);
            let trimmed = content.trim_end_matches([' ', '\t']);
            if trimmed.len() < content.len() {
                // Fits in u32: the lexer already rejects larger sources.
                #[allow(clippy::cast_possible_truncation)]
                let span = Span::new(
                    (line_start + trimmed.len()) as u32,
                    (line_start + content.len()) as u32,
                );
                let diagnostic = ctx.report("trailing whitespace", span);
                diagnostic.help = Some("remove the spaces at the end of the line".to_owned());
                diagnostic.fix = Some(Fix {
                    span,
                    replacement: String::new(),
                });
            }
            line_start += line.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use kosmetic_diagnostics::Span;

    use crate::{Config, run};

    fn check(source: &str) -> Vec<Span> {
        let parse = kosmetic_syntax::parse(source);
        run(source, &parse, &Config::default())
            .into_iter()
            .map(|diagnostic| diagnostic.span)
            .collect()
    }

    #[test]
    fn flags_spaces_and_tabs() {
        assert_eq!(
            check("a = 1  \nb = 2\t\n"),
            vec![Span::new(5, 7), Span::new(13, 14)]
        );
    }

    #[test]
    fn ignores_clean_lines_and_crlf() {
        assert!(check("a = 1\r\nb = 2\n").is_empty());
    }

    #[test]
    fn can_be_turned_off() {
        let mut config = Config::default();
        config
            .rules
            .insert("style/no-trailing-whitespace".to_owned(), crate::Level::Off);
        let source = "a = 1  \n";
        let parse = kosmetic_syntax::parse(source);
        assert!(run(source, &parse, &config).is_empty());
    }
}
