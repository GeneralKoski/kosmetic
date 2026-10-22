//! Lint rules and the engine that runs them.
//!
//! Each rule is a type implementing [`Rule`], registered in [`all_rules`].
//! Its level comes from the [`Config`], so users can turn rules off or make
//! them fail the build.

mod config;
mod fix;
mod schema;
mod semantic;
mod style;

pub use config::{Config, Level};
pub use fix::{FixError, Fixed, fix_source};

use std::path::Path;

use kosmetic_diagnostics::{Diagnostic, Severity, Span};
use kosmetic_syntax::{Parse, Table, build_model};

/// What a file is, which decides whether the schema rules apply to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileKind {
    /// Any TOML file: only the syntax, semantic and style rules run.
    #[default]
    Other,
    /// A `Cargo.toml` manifest.
    CargoManifest,
    /// A `pyproject.toml` file.
    PyProject,
}

impl FileKind {
    /// Recognizes a file by its name.
    pub fn from_path(path: &Path) -> Self {
        match path.file_name().and_then(|name| name.to_str()) {
            Some("Cargo.toml") => Self::CargoManifest,
            Some("pyproject.toml") => Self::PyProject,
            _ => Self::Other,
        }
    }
}

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
    /// Tokens and tree of the file.
    pub parse: &'a Parse,
    /// The values of the file, with the spans of keys and values.
    pub model: &'a Table,
    /// What kind of file is being checked.
    pub file: FileKind,
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
    vec![
        Box::new(style::NoTrailingWhitespace),
        Box::new(style::FinalNewline),
        Box::new(style::NoMultipleBlankLines),
        Box::new(style::ConsistentLineEndings),
        Box::new(style::SpacesAroundEquals),
        Box::new(style::CommentSpace),
        Box::new(style::PreferBareKeys),
        Box::new(style::SortedKeys),
        Box::new(semantic::MixedArrayTypes),
        Box::new(schema::CargoUnknownKey),
        Box::new(schema::CargoMissingField),
        Box::new(schema::CargoInvalidValue),
        Box::new(schema::CargoWildcardDependency),
        Box::new(schema::CargoPublishMetadata),
        Box::new(schema::PyprojectUnknownKey),
        Box::new(schema::PyprojectMissingField),
        Box::new(schema::PyprojectInvalidValue),
    ]
}

/// Runs every enabled rule on a parsed file and returns their diagnostics,
/// sorted by position. `file` selects the schema rules that apply.
pub fn run(source: &str, parse: &Parse, config: &Config, file: FileKind) -> Vec<Diagnostic> {
    let (model, _) = build_model(source, &parse.tree);
    let mut diagnostics = Vec::new();
    for rule in all_rules() {
        let meta = rule.meta();
        let Some(severity) = config.level_for(meta).severity() else {
            continue;
        };
        let mut ctx = RuleContext {
            source,
            parse,
            model: &model,
            file,
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
pub(crate) mod testing {
    use kosmetic_diagnostics::Diagnostic;

    use crate::{Config, FileKind, Level, run};

    /// Diagnostics of one rule, at its default level or the given one.
    pub(crate) fn lint(code: &str, source: &str, level: Option<Level>) -> Vec<Diagnostic> {
        lint_file(code, source, level, FileKind::Other)
    }

    /// Like [`lint`], for a file of the given kind.
    pub(crate) fn lint_file(
        code: &str,
        source: &str,
        level: Option<Level>,
        file: FileKind,
    ) -> Vec<Diagnostic> {
        let mut config = Config::default();
        if let Some(level) = level {
            config.rules.insert(code.to_owned(), level);
        }
        let parse = kosmetic_syntax::parse(source);
        run(source, &parse, &config, file)
            .into_iter()
            .filter(|diagnostic| diagnostic.code == code)
            .collect()
    }

    /// The source with the fixes of one rule applied. They must not overlap.
    pub(crate) fn fixed(code: &str, source: &str) -> String {
        let mut out = source.to_owned();
        let mut fixes: Vec<_> = lint(code, source, Some(Level::Warn))
            .into_iter()
            .filter_map(|diagnostic| diagnostic.fix)
            .collect();
        fixes.sort_by_key(|fix| fix.span);
        for fix in fixes.iter().rev() {
            out.replace_range(fix.span.range(), &fix.replacement);
        }
        out
    }
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
