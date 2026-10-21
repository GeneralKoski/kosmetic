use std::collections::BTreeMap;

use kosmetic_diagnostics::{Diagnostic, Severity, Span};

use crate::literal::{DateTimeKind, datetime_kind, string_value};
use crate::{Node, NodeKind, TokenKind};

/// A TOML value, decoded from the tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Any of the four string kinds, escapes resolved.
    String(String),
    /// A 64-bit signed integer.
    Integer(i64),
    /// An IEEE 754 binary64 float.
    Float(f64),
    /// `true` or `false`.
    Boolean(bool),
    /// One of the four date-time types.
    DateTime(DateTime),
    /// A `[...]` array value.
    Array(Vec<Value>),
    /// An array built from `[[header]]` sections.
    ArrayOfTables(Vec<Table>),
    /// A table, from a header, dotted keys or an inline table.
    Table(Table),
}

/// A date-time, checked for ranges and normalized to RFC 3339: `T` as the
/// separator and an uppercase `Z`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateTime {
    /// Which of the four types it is.
    pub kind: DateTimeKind,
    /// Normalized text.
    pub text: String,
}

/// How a table came to exist. It decides whether it can be extended later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableOrigin {
    /// The document itself.
    Root,
    /// Defined by a `[header]` or as an element of a `[[header]]`.
    Header,
    /// Created as a parent of a header, `a` in `[a.b]`. A later `[a]` may define it.
    Implicit,
    /// Created by a dotted key, `a` in `a.b = 1`. Only more dotted keys may extend it.
    Dotted,
    /// An inline table: sealed, nothing can be added to it.
    Inline,
}

/// A table: its keys, sorted, and where it comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// How the table was created.
    pub origin: TableOrigin,
    /// Keys and values.
    pub entries: BTreeMap<String, Value>,
    /// Key and value span of each entry, for diagnostics.
    spans: BTreeMap<String, (Span, Span)>,
}

impl Table {
    fn new(origin: TableOrigin) -> Self {
        Self {
            origin,
            entries: BTreeMap::new(),
            spans: BTreeMap::new(),
        }
    }

    fn insert(&mut self, key: &str, key_span: Span, value_span: Span, value: Value) {
        self.entries.insert(key.to_owned(), value);
        self.spans.insert(key.to_owned(), (key_span, value_span));
    }

    /// Where `key` was first defined: the last segment of its key.
    pub fn key_span(&self, key: &str) -> Option<Span> {
        self.spans.get(key).map(|&(key, _)| key)
    }

    /// Where the value of `key` is. Tables created by a header or a dotted
    /// key have no value of their own and point at their key instead.
    pub fn value_span(&self, key: &str) -> Option<Span> {
        self.spans.get(key).map(|&(_, value)| value)
    }
}

/// Builds the values of a parsed document and checks what the grammar alone
/// cannot: duplicate keys, redefined tables, integers out of range and
/// impossible dates. Nodes with syntax errors are skipped, so this can run on
/// any tree.
pub fn build_model(source: &str, tree: &Node) -> (Table, Vec<Diagnostic>) {
    let mut builder = Builder {
        source,
        diagnostics: Vec::new(),
    };
    let mut root = Table::new(TableOrigin::Root);
    for node in tree.nodes() {
        match node.kind {
            NodeKind::KeyValue => builder.key_value(&mut root, node),
            NodeKind::Table | NodeKind::ArrayTable => {
                // Entries of an invalid header still get their duplicate checks.
                let mut scratch = Table::new(TableOrigin::Header);
                let target = builder.header(&mut root, node).unwrap_or(&mut scratch);
                for entry in node.nodes().filter(|n| n.kind == NodeKind::KeyValue) {
                    builder.key_value(target, entry);
                }
            }
            _ => {}
        }
    }
    (root, builder.diagnostics)
}

struct Builder<'a> {
    source: &'a str,
    diagnostics: Vec<Diagnostic>,
}

impl Builder<'_> {
    fn error(&mut self, code: &'static str, message: String, span: Span) {
        self.diagnostics
            .push(Diagnostic::new(code, Severity::Error, message, span));
    }

    fn conflict(&mut self, code: &'static str, message: String, span: Span, first: Option<Span>) {
        let mut diagnostic = Diagnostic::new(code, Severity::Error, message, span);
        if let Some(first) = first {
            diagnostic = diagnostic.with_label(first, "first defined here");
        }
        self.diagnostics.push(diagnostic);
    }

    fn key(&self, key: &Node) -> Vec<(String, Span)> {
        key.tokens()
            .filter(|token| !matches!(token.kind, TokenKind::Dot | TokenKind::Whitespace))
            .map(|token| {
                let text = &self.source[token.span.range()];
                (string_value(token.kind, text), token.span)
            })
            .collect()
    }

    /// Resolves a `[header]` or `[[header]]` to the table its entries go into.
    fn header<'t>(&mut self, root: &'t mut Table, node: &Node) -> Option<&'t mut Table> {
        let header = node.nodes().next()?;
        let key = self.key(header.child(NodeKind::Key)?);
        let ((name, span), parents) = key.split_last()?;
        let mut table = root;
        for (part, part_span) in parents {
            table = self.descend(table, part, *part_span, true)?;
        }
        let first = table.key_span(name);

        if node.kind == NodeKind::ArrayTable {
            match table.entries.get(name) {
                None => table.insert(name, *span, *span, Value::ArrayOfTables(Vec::new())),
                Some(Value::ArrayOfTables(_)) => {}
                Some(_) => {
                    let message =
                        format!("`{name}` is already defined and is not an array of tables");
                    self.conflict("semantic/table-redefined", message, *span, first);
                    return None;
                }
            }
            return match table.entries.get_mut(name) {
                Some(Value::ArrayOfTables(tables)) => {
                    tables.push(Table::new(TableOrigin::Header));
                    tables.last_mut()
                }
                _ => None,
            };
        }

        match table.entries.get(name) {
            None => table.insert(
                name,
                *span,
                *span,
                Value::Table(Table::new(TableOrigin::Header)),
            ),
            Some(Value::Table(existing)) if existing.origin == TableOrigin::Implicit => {}
            Some(_) => {
                self.conflict(
                    "semantic/table-redefined",
                    format!("table `{name}` is already defined"),
                    *span,
                    first,
                );
                return None;
            }
        }
        match table.entries.get_mut(name) {
            Some(Value::Table(table)) => {
                table.origin = TableOrigin::Header;
                Some(table)
            }
            _ => None,
        }
    }

    /// Walks into the table `name` of a key path, creating it if needed.
    /// Headers go through any table but inline ones, and into the last
    /// element of an array of tables; dotted keys only extend tables that
    /// dotted keys created.
    fn descend<'t>(
        &mut self,
        table: &'t mut Table,
        name: &str,
        span: Span,
        header: bool,
    ) -> Option<&'t mut Table> {
        let allowed = match table.entries.get(name) {
            None => {
                let origin = if header {
                    TableOrigin::Implicit
                } else {
                    TableOrigin::Dotted
                };
                table.insert(name, span, span, Value::Table(Table::new(origin)));
                true
            }
            Some(Value::Table(existing)) if header => existing.origin != TableOrigin::Inline,
            Some(Value::Table(existing)) => existing.origin == TableOrigin::Dotted,
            Some(Value::ArrayOfTables(_)) => header,
            Some(_) => false,
        };
        if !allowed {
            let first = table.key_span(name);
            if header {
                let message =
                    format!("`{name}` is already defined and cannot be extended by a header");
                self.conflict("semantic/table-redefined", message, span, first);
            } else {
                let message =
                    format!("`{name}` is already defined and cannot be extended by a dotted key");
                self.conflict("semantic/duplicate-key", message, span, first);
            }
            return None;
        }
        match table.entries.get_mut(name) {
            Some(Value::Table(table)) => Some(table),
            Some(Value::ArrayOfTables(tables)) => tables.last_mut(),
            _ => None,
        }
    }

    fn key_value(&mut self, table: &mut Table, node: &Node) {
        let Some(key) = node.child(NodeKind::Key) else {
            return;
        };
        let key = self.key(key);
        let Some(((name, span), parents)) = key.split_last() else {
            return;
        };
        let Some(value_node) = node.nodes().find(|n| n.kind.is_value()) else {
            return;
        };
        let Some(value) = self.value(value_node) else {
            return;
        };
        let mut table = table;
        for (part, part_span) in parents {
            let Some(inner) = self.descend(table, part, *part_span, false) else {
                return;
            };
            table = inner;
        }
        if table.entries.contains_key(name) {
            let first = table.key_span(name);
            self.conflict(
                "semantic/duplicate-key",
                format!("duplicate key `{name}`"),
                *span,
                first,
            );
            return;
        }
        table.insert(name, *span, value_node.span, value);
    }

    fn value(&mut self, node: &Node) -> Option<Value> {
        let text = node.text(self.source);
        match node.kind {
            NodeKind::String => {
                let token = node.tokens().next()?;
                Some(Value::String(string_value(token.kind, text)))
            }
            NodeKind::Integer => self.integer(text, node.span),
            NodeKind::Float => float(text).map(Value::Float),
            NodeKind::Boolean => Some(Value::Boolean(text == "true")),
            NodeKind::DateTime => self.datetime(text, node.span),
            NodeKind::Array => {
                let mut values = Vec::new();
                for child in node.nodes() {
                    if let Some(value) = self.value(child) {
                        values.push(value);
                    }
                }
                Some(Value::Array(values))
            }
            NodeKind::InlineTable => {
                let mut table = Table::new(TableOrigin::Inline);
                for entry in node.nodes().filter(|n| n.kind == NodeKind::KeyValue) {
                    self.key_value(&mut table, entry);
                }
                Some(Value::Table(table))
            }
            _ => None,
        }
    }

    fn integer(&mut self, text: &str, span: Span) -> Option<Value> {
        let digits: String = text.chars().filter(|&c| c != '_').collect();
        let parsed = match digits.get(..2) {
            Some("0x") => i64::from_str_radix(&digits[2..], 16),
            Some("0o") => i64::from_str_radix(&digits[2..], 8),
            Some("0b") => i64::from_str_radix(&digits[2..], 2),
            _ => digits.parse(),
        };
        if let Ok(value) = parsed {
            return Some(Value::Integer(value));
        }
        self.error(
            "semantic/integer-overflow",
            format!("integer `{text}` does not fit in 64 bits"),
            span,
        );
        None
    }

    fn datetime(&mut self, text: &str, span: Span) -> Option<Value> {
        let kind = datetime_kind(text)?;
        if !datetime_in_range(text, kind) {
            self.error(
                "semantic/invalid-datetime",
                format!("`{text}` is not a valid date or time"),
                span,
            );
            return None;
        }
        let mut text = text.to_owned();
        if matches!(kind, DateTimeKind::Offset | DateTimeKind::Local) {
            text.replace_range(10..11, "T");
        }
        if text.ends_with('z') {
            text.pop();
            text.push('Z');
        }
        Some(Value::DateTime(DateTime { kind, text }))
    }
}

fn float(text: &str) -> Option<f64> {
    let digits: String = text.chars().filter(|&c| c != '_').collect();
    match digits.trim_start_matches(['+', '-']) {
        "inf" if digits.starts_with('-') => Some(f64::NEG_INFINITY),
        "inf" => Some(f64::INFINITY),
        "nan" => Some(f64::NAN),
        _ => digits.parse().ok(),
    }
}

/// Field ranges of a date-time whose shape [`datetime_kind`] already checked.
fn datetime_in_range(text: &str, kind: DateTimeKind) -> bool {
    let n = |at: usize, len: usize| -> u32 {
        text.get(at..at + len)
            .and_then(|digits| digits.parse().ok())
            .unwrap_or(u32::MAX)
    };
    let date = || {
        let (year, month, day) = (n(0, 4), n(5, 2), n(8, 2));
        let days = match month {
            2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        (1..=12).contains(&month) && (1..=days).contains(&day)
    };
    let time = |at: usize| n(at, 2) < 24 && n(at + 3, 2) < 60 && n(at + 6, 2) < 60;
    let offset = || {
        let len = text.len();
        text.ends_with(['Z', 'z']) || (n(len - 5, 2) < 24 && n(len - 2, 2) < 60)
    };
    match kind {
        DateTimeKind::Date => date(),
        DateTimeKind::Time => time(0),
        DateTimeKind::Local => date() && time(11),
        DateTimeKind::Offset => date() && time(11) && offset(),
    }
}

#[cfg(test)]
mod tests {
    use kosmetic_diagnostics::Span;

    use super::{Table, TableOrigin, Value, build_model};
    use crate::parse;

    fn model(source: &str) -> (Table, Vec<&'static str>) {
        let tree = parse(source).tree;
        let (table, diagnostics) = build_model(source, &tree);
        (table, diagnostics.into_iter().map(|d| d.code).collect())
    }

    fn codes(source: &str) -> Vec<&'static str> {
        model(source).1
    }

    /// Follows a dotted path, taking the last element of arrays of tables.
    fn get<'a>(table: &'a Table, path: &str) -> &'a Value {
        let (last, parents) = path
            .rsplit_once('.')
            .map_or((path, None), |(a, b)| (b, Some(a)));
        let mut table = table;
        for part in parents.into_iter().flat_map(|p| p.split('.')) {
            table = match &table.entries[part] {
                Value::Table(table) => table,
                Value::ArrayOfTables(tables) => tables.last().expect("non-empty"),
                other => panic!("{part} is {other:?}"),
            };
        }
        &table.entries[last]
    }

    #[test]
    fn decodes_values() {
        let source = r#"
s = "a\tb"
ml = """
x"""
i = 0xff
big = -9_223_372_036_854_775_808
f = 6.626e-34
ninf = -inf
b = true
odt = 1979-05-27 07:32:00z
ld = 2024-02-29
arr = [1, "two", [3]]
it = { x.y = 1 }
"#;
        let (table, codes) = model(source);
        assert_eq!(codes, Vec::<&str>::new());
        assert_eq!(get(&table, "s"), &Value::String("a\tb".into()));
        assert_eq!(get(&table, "ml"), &Value::String("x".into()));
        assert_eq!(get(&table, "i"), &Value::Integer(255));
        assert_eq!(get(&table, "big"), &Value::Integer(i64::MIN));
        assert_eq!(get(&table, "f"), &Value::Float(6.626e-34));
        assert_eq!(get(&table, "ninf"), &Value::Float(f64::NEG_INFINITY));
        assert_eq!(get(&table, "b"), &Value::Boolean(true));
        let Value::DateTime(odt) = get(&table, "odt") else {
            panic!("not a date-time");
        };
        assert_eq!(odt.text, "1979-05-27T07:32:00Z");
        assert!(matches!(get(&table, "arr"), Value::Array(values) if values.len() == 3));
        assert_eq!(get(&table, "it.x.y"), &Value::Integer(1));
    }

    #[test]
    fn builds_tables() {
        let source = "a.b = 1\n[t.u]\nv = 2\n[t]\nw = 3\n[[arr]]\nx = 1\n[[arr]]\n[arr.sub]\ny = 2\n[fruit]\napple.color = 1\n[fruit.apple.texture]\nsmooth = true\n";
        let (table, codes) = model(source);
        assert_eq!(codes, Vec::<&str>::new());
        assert_eq!(get(&table, "a.b"), &Value::Integer(1));
        assert_eq!(get(&table, "t.u.v"), &Value::Integer(2));
        assert_eq!(get(&table, "t.w"), &Value::Integer(3));
        let Value::ArrayOfTables(arr) = get(&table, "arr") else {
            panic!("not an array of tables");
        };
        assert_eq!(arr.len(), 2);
        assert_eq!(get(&table, "arr.sub.y"), &Value::Integer(2));
        assert_eq!(
            get(&table, "fruit.apple.texture.smooth"),
            &Value::Boolean(true)
        );
        let Value::Table(t) = get(&table, "t") else {
            panic!("not a table");
        };
        assert_eq!(t.origin, TableOrigin::Header);
    }

    #[test]
    fn reports_duplicate_keys() {
        for source in [
            "a = 1\na = 2",
            "a = 1\na.b = 2",
            "a = {b = 1}\na.c = 2",
            "[a.b]\n[a]\nb.c = 1",
            "x = {a = 1, a = 2}",
            "'a' = 1\n\"a\" = 2",
        ] {
            assert_eq!(codes(source), vec!["semantic/duplicate-key"], "{source:?}");
        }
    }

    #[test]
    fn reports_redefined_tables() {
        for source in [
            "[a]\n[a]",
            "a.b = 1\n[a]",
            "[fruit]\napple.color = 1\n[fruit.apple]",
            "a = {}\n[a.b]",
            "a = 1\n[a.b]",
            "a = []\n[[a]]",
            "[a]\n[[a]]",
            "[[a]]\n[a]",
        ] {
            assert_eq!(
                codes(source),
                vec!["semantic/table-redefined"],
                "{source:?}"
            );
        }
    }

    #[test]
    fn checks_ranges() {
        assert_eq!(
            codes("a = 9223372036854775808"),
            vec!["semantic/integer-overflow"]
        );
        assert_eq!(
            codes("a = 0x8000000000000000"),
            vec!["semantic/integer-overflow"]
        );
        for source in [
            "a = 2023-02-29",
            "a = 2024-13-01",
            "a = 2024-04-31",
            "a = 24:00:00",
            "a = 12:60:00",
            "a = 1979-05-27T07:32:00+24:00",
            "a = 0000-00-00",
        ] {
            assert_eq!(
                codes(source),
                vec!["semantic/invalid-datetime"],
                "{source:?}"
            );
        }
        assert_eq!(
            codes("a = 2000-02-29\nb = 23:59:59.999\nc = 1979-05-27T00:00:00-23:59"),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn records_key_and_value_spans() {
        let (table, _) = model("[t]\na.b = \"x\"\n");
        let Value::Table(t) = &table.entries["t"] else {
            panic!("not a table");
        };
        let Value::Table(a) = &t.entries["a"] else {
            panic!("not a table");
        };
        assert_eq!(table.key_span("t"), Some(Span::new(1, 2)));
        assert_eq!(a.key_span("b"), Some(Span::new(6, 7)));
        assert_eq!(a.value_span("b"), Some(Span::new(10, 13)));
    }

    #[test]
    fn survives_broken_trees() {
        for source in ["[", "[[a", "a.", "= 1", "a = [1,", "a = {b.", "[a.b]]\nc"] {
            let _ = model(source);
        }
    }
}
