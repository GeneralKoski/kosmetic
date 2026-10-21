use std::collections::HashMap;

use kosmetic_diagnostics::Severity;

use crate::RuleMeta;

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

/// User configuration: per-rule level overrides.
///
/// TODO(M3): load it from `.kosmetic.toml`, parsed with kosmetic's own parser,
/// and lint that file with kosmetic itself.
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
}
