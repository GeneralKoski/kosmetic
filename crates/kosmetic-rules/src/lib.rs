//! Lint rules and the engine that runs them.
//!
//! Each rule is a type implementing [`Rule`], registered in [`all_rules`].
//! Its level comes from the [`Config`], so users can turn rules off or make
//! them fail the build.

mod config;
mod style;

pub use config::{Config, Level};

use kosmetic_diagnostics::{Diagnostic, Severity, Span};
use kosmetic_syntax::Parse;

/// Static description of a rule.
#[derive(Debug, Clone, Copy)]
pub struct RuleMeta {
    /// Stable identifier, `category/name`, used in output and in the config file.
    pub code: &'static str,
    /// One-line description, shown by `kosmetic rules`.
    pub description: &'static str,
    /// Level used when the config does not mention the rule.
    pub default_level: Level,
}

/// What a rule sees while it runs, and where it reports problems.
pub struct RuleContext<'a> {
    /// The raw source text.
    pub source: &'a str,
    /// Tokens (and later the tree) of the file.
    pub parse: &'a Parse,
    meta: &'static RuleMeta,
    severity: Severity,
    diagnostics: Vec<Diagnostic>,
}

impl RuleContext<'_> {
    /// Reports a problem with the rule's code and configured severity.
    /// Returns the diagnostic so the caller can add a label, help or fix.
    ///
    /// # Panics
    ///
    /// Never: the diagnostic is pushed right before being returned.
    pub fn report(&mut self, message: impl Into<String>, span: Span) -> &mut Diagnostic {
        self.diagnostics.push(Diagnostic::new(
            self.meta.code,
            self.severity,
            message,
            span,
        ));
        self.diagnostics.last_mut().expect("just pushed")
    }
}

/// A lint rule.
pub trait Rule {
    /// Static information about the rule.
    fn meta(&self) -> &'static RuleMeta;
    /// Inspects the file and reports problems through `ctx`.
    fn check(&self, ctx: &mut RuleContext<'_>);
}

/// Every rule kosmetic knows about.
pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![Box::new(style::NoTrailingWhitespace)]
}

/// Runs every enabled rule on a parsed file and returns their diagnostics,
/// sorted by position.
pub fn run(source: &str, parse: &Parse, config: &Config) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for rule in all_rules() {
        let meta = rule.meta();
        let Some(severity) = config.level_for(meta).severity() else {
            continue;
        };
        let mut ctx = RuleContext {
            source,
            parse,
            meta,
            severity,
            diagnostics: Vec::new(),
        };
        rule.check(&mut ctx);
        diagnostics.append(&mut ctx.diagnostics);
    }
    diagnostics.sort_by_key(|diagnostic| diagnostic.span);
    diagnostics
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::all_rules;

    #[test]
    fn rule_codes_are_unique_and_categorized() {
        let mut seen = HashSet::new();
        for rule in all_rules() {
            let code = rule.meta().code;
            assert!(code.contains('/'), "{code} must be `category/name`");
            assert!(seen.insert(code), "{code} is registered twice");
        }
    }
}
