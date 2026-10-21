use std::collections::HashMap;

use kosmetic_diagnostics::{Diagnostic, Severity, Span};
use kosmetic_syntax::{Value, build_model, parse};

use crate::{RuleMeta, all_rules};

/// How a rule is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// The rule does not run.
    Off,
    /// Problems are reported but do not fail the run.
    Warn,
    /// Problems fail the run (exit code 1).
    Error,
}

impl Level {
    /// Severity of the diagnostics produced at this level, `None` when off.
    pub fn severity(self) -> Option<Severity> {
        match self {
            Self::Off => None,
            Self::Warn => Some(Severity::Warning),
            Self::Error => Some(Severity::Error),
        }
    }
}

/// User configuration: per-rule level overrides, read from `.kosmetic.toml`.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Overrides keyed by rule code, for example `style/no-trailing-whitespace`.
    pub rules: HashMap<String, Level>,
}

impl Config {
    /// Level of a rule: the override if present, otherwise the rule's default.
    pub fn level_for(&self, meta: &RuleMeta) -> Level {
        self.rules
            .get(meta.code)
            .copied()
            .unwrap_or(meta.default_level)
    }

    /// Reads a `.kosmetic.toml` with kosmetic's own parser:
    ///
    /// ```toml
    /// [rules]
    /// "style/sorted-keys" = "warn"
    /// "style/comment-space" = "off"
    /// ```
    ///
    /// Never fails: syntax errors, unknown rules and bad levels come back as
    /// diagnostics on the file, and the entries that are valid still apply.
    pub fn from_toml(source: &str) -> (Self, Vec<Diagnostic>) {
        let parse = parse(source);
        let (root, semantic) = build_model(source, &parse.tree);
        let mut diagnostics = parse.diagnostics;
        diagnostics.extend(semantic);
        let error = |code, message: String, span: Option<Span>| {
            Diagnostic::new(code, Severity::Error, message, span.unwrap_or_default())
        };

        let codes: Vec<_> = all_rules().iter().map(|rule| rule.meta().code).collect();
        let mut config = Self::default();
        for (key, value) in &root.entries {
            if key != "rules" {
                diagnostics.push(
                    error(
                        "schema/unknown-key",
                        format!("unknown key `{key}`"),
                        root.key_span(key),
                    )
                    .with_help("the only key is `[rules]`"),
                );
                continue;
            }
            let Value::Table(rules) = value else {
                let message = "`rules` must be a table".to_owned();
                diagnostics.push(error("schema/invalid-type", message, root.value_span(key)));
                continue;
            };
            for (code, level) in &rules.entries {
                if !codes.contains(&code.as_str()) {
                    diagnostics.push(
                        error(
                            "schema/unknown-key",
                            format!("unknown rule `{code}`"),
                            rules.key_span(code),
                        )
                        .with_help(
                            "`kosmetic rules` lists every rule; quote codes that contain `/`",
                        ),
                    );
                    continue;
                }
                let level = match level {
                    Value::String(level) if level == "off" => Level::Off,
                    Value::String(level) if level == "warn" => Level::Warn,
                    Value::String(level) if level == "error" => Level::Error,
                    _ => {
                        diagnostics.push(
                            error(
                                "schema/invalid-value",
                                format!("invalid level for `{code}`"),
                                rules.value_span(code),
                            )
                            .with_help("use \"off\", \"warn\" or \"error\""),
                        );
                        continue;
                    }
                };
                config.rules.insert(code.clone(), level);
            }
        }
        (config, diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, Level};

    fn codes(source: &str) -> Vec<&'static str> {
        Config::from_toml(source)
            .1
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn reads_rule_levels() {
        let (config, diagnostics) = Config::from_toml(
            "[rules]\n\"style/no-trailing-whitespace\" = \"off\"\n\"style/sorted-keys\" = \"error\"\n",
        );
        assert_eq!(diagnostics, Vec::new());
        assert_eq!(config.rules["style/no-trailing-whitespace"], Level::Off);
        assert_eq!(config.rules["style/sorted-keys"], Level::Error);
    }

    #[test]
    fn reports_problems_and_keeps_valid_entries() {
        let source = "colors = true\n[rules]\n\"style/nope\" = \"off\"\n\"style/final-newline\" = \"loud\"\n\"style/comment-space\" = \"off\"\n";
        assert_eq!(
            codes(source),
            vec![
                "schema/unknown-key",
                "schema/invalid-value",
                "schema/unknown-key"
            ]
        );
        assert_eq!(
            Config::from_toml(source).0.rules["style/comment-space"],
            Level::Off
        );
    }

    #[test]
    fn reports_syntax_errors_of_the_file() {
        assert_eq!(codes("[rules\n"), vec!["syntax/unclosed-delimiter"]);
        assert_eq!(codes("rules = 1\n"), vec!["schema/invalid-type"]);
    }
}
