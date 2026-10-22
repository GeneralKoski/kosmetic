//! Schema rules: valid TOML that a tool reading the file will not accept.
//!
//! They run only on the files they know, `Cargo.toml` and `pyproject.toml`,
//! and read the values of the document rather than its tokens. The grammars
//! of versions and requirements are checked by hand-written validators.

mod cargo;
mod pep;
mod pyproject;
#[cfg(test)]
mod rule_tests;
mod semver;

pub(crate) use cargo::{
    CargoInvalidValue, CargoMissingField, CargoPublishMetadata, CargoUnknownKey,
    CargoWildcardDependency,
};
pub(crate) use pyproject::{PyprojectInvalidValue, PyprojectMissingField, PyprojectUnknownKey};

use kosmetic_diagnostics::Span;
use kosmetic_syntax::{Node, NodeKind, Table, Value};

use crate::RuleContext;

/// The type of a value, as a user would call it.
fn type_name(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "a string",
        Value::Integer(_) => "an integer",
        Value::Float(_) => "a float",
        Value::Boolean(_) => "a boolean",
        Value::DateTime(_) => "a date-time",
        Value::Array(_) => "an array",
        Value::ArrayOfTables(_) => "an array of tables",
        Value::Table(_) => "a table",
    }
}

/// What a key is expected to hold.
#[derive(Debug, Clone, Copy)]
enum Shape {
    Str,
    Bool,
    StrArray,
    StrOrBool,
    StrOrTable,
    BoolOrStrArray,
    Table,
}

impl Shape {
    fn describe(self) -> &'static str {
        match self {
            Self::Str => "a string",
            Self::Bool => "a boolean",
            Self::StrArray => "an array of strings",
            Self::StrOrBool => "a string or a boolean",
            Self::StrOrTable => "a string or a table",
            Self::BoolOrStrArray => "a boolean or an array of strings",
            Self::Table => "a table",
        }
    }

    fn fits(self, value: &Value) -> bool {
        match self {
            Self::Str => matches!(value, Value::String(_)),
            Self::Bool => matches!(value, Value::Boolean(_)),
            Self::StrArray => strings(value).is_some(),
            Self::StrOrBool => matches!(value, Value::String(_) | Value::Boolean(_)),
            Self::StrOrTable => matches!(value, Value::String(_) | Value::Table(_)),
            Self::BoolOrStrArray => matches!(value, Value::Boolean(_)) || strings(value).is_some(),
            Self::Table => matches!(value, Value::Table(_)),
        }
    }
}

/// The items of an array made only of strings.
fn strings(value: &Value) -> Option<Vec<&str>> {
    let Value::Array(items) = value else {
        return None;
    };
    items
        .iter()
        .map(|item| match item {
            Value::String(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Every table a value stands for: a table, an array of tables from
/// `[[headers]]`, or an array of inline tables.
fn tables(value: &Value) -> Vec<&Table> {
    match value {
        Value::Table(table) => vec![table],
        Value::ArrayOfTables(tables) => tables.iter().collect(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| match item {
                Value::Table(table) => Some(table),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The table stored under `key`, if it is one.
fn table<'a>(parent: &'a Table, key: &str) -> Option<&'a Table> {
    match parent.entries.get(key) {
        Some(Value::Table(table)) => Some(table),
        _ => None,
    }
}

/// The string stored under `key`, if it is one.
fn string<'a>(parent: &'a Table, key: &str) -> Option<&'a str> {
    match parent.entries.get(key) {
        Some(Value::String(text)) => Some(text),
        _ => None,
    }
}

/// Spans of the items of the array whose own span is `array`.
fn item_spans(tree: &Node, array: Span) -> Vec<Span> {
    tree.descendants()
        .find(|node| node.kind == NodeKind::Array && node.span == array)
        .map(|node| {
            node.nodes()
                .filter(|item| item.kind.is_value())
                .map(|item| item.span)
                .collect()
        })
        .unwrap_or_default()
}

/// Edit distance between two names.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<_> = b.chars().collect();
    let mut row: Vec<_> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != *cb))
                .min(row[j] + 1)
                .min(above + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The known name closest to `name`, if it is close enough to be a typo.
fn suggestion<'a>(name: &str, known: &[&'a str]) -> Option<&'a str> {
    let limit = (name.chars().count() / 3).clamp(1, 3);
    known
        .iter()
        .map(|candidate| (distance(name, candidate), *candidate))
        .filter(|(distance, _)| *distance <= limit)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

/// Reports every key of `table` that is not in `known`.
fn report_unknown_keys(ctx: &mut RuleContext<'_>, table: &Table, place: &str, known: &[&str]) {
    for key in table
        .entries
        .keys()
        .filter(|key| !known.contains(&key.as_str()))
    {
        let span = table.key_span(key).unwrap_or_default();
        let help = match suggestion(key, known) {
            Some(close) => format!("did you mean `{close}`?"),
            None => "check the spelling of the key".to_owned(),
        };
        ctx.report(format!("unknown key `{key}` in {place}"), span)
            .help = Some(help);
    }
}

/// Reports `key` if it holds something other than `shape`. Returns whether
/// the key is absent or fine.
fn check_shape(
    ctx: &mut RuleContext<'_>,
    table: &Table,
    path: &str,
    key: &str,
    shape: Shape,
) -> bool {
    let Some(value) = table.entries.get(key) else {
        return true;
    };
    if shape.fits(value) {
        return true;
    }
    let span = table.value_span(key).unwrap_or_default();
    let found = match (shape, value) {
        (Shape::StrArray | Shape::BoolOrStrArray, Value::Array(_)) => {
            "an array with items that are not strings"
        }
        _ => type_name(value),
    };
    let diagnostic = ctx.report(
        format!("`{path}` must be {}, found {found}", shape.describe()),
        span,
    );
    if let (Shape::Str, Value::Integer(number)) = (shape, value) {
        diagnostic.help = Some(format!("quote the value: \"{number}\""));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{distance, suggestion};

    #[test]
    fn edit_distance() {
        assert_eq!(distance("", ""), 0);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("edition", "editon"), 1);
    }

    #[test]
    fn suggests_close_names_only() {
        let known = ["edition", "license", "version"];
        assert_eq!(suggestion("editon", &known), Some("edition"));
        assert_eq!(suggestion("licence", &known), Some("license"));
        assert_eq!(suggestion("homepage", &known), None);
        assert_eq!(suggestion("x", &known), None);
    }
}
