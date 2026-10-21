//! Decoder for the `toml-test` suite: reads TOML from stdin and prints it as
//! JSON with tagged scalars, `{"type": "integer", "value": "42"}`. Exits with
//! 1 and the diagnostics on stderr if the input is not valid TOML.

use std::io::Read;
use std::process::ExitCode;

use kosmetic_syntax::{DateTimeKind, Table, Value, build_model, parse};
use serde_json::{Map, Value as Json, json};

fn main() -> ExitCode {
    let mut bytes = Vec::new();
    if let Err(err) = std::io::stdin().read_to_end(&mut bytes) {
        eprintln!("cannot read stdin: {err}");
        return ExitCode::from(2);
    }
    let Ok(source) = String::from_utf8(bytes) else {
        eprintln!("input is not valid UTF-8");
        return ExitCode::FAILURE;
    };

    let parse = parse(&source);
    let (table, semantic) = build_model(&source, &parse.tree);
    let diagnostics: Vec<_> = parse.diagnostics.iter().chain(&semantic).collect();
    if !diagnostics.is_empty() {
        for diagnostic in diagnostics {
            eprintln!("{}: {}", diagnostic.code, diagnostic.message);
        }
        return ExitCode::FAILURE;
    }
    println!("{}", table_json(&table));
    ExitCode::SUCCESS
}

fn table_json(table: &Table) -> Json {
    Json::Object(
        table
            .entries
            .iter()
            .map(|(key, value)| (key.clone(), value_json(value)))
            .collect::<Map<_, _>>(),
    )
}

fn value_json(value: &Value) -> Json {
    let (kind, text) = match value {
        Value::String(text) => ("string", text.clone()),
        Value::Integer(n) => ("integer", n.to_string()),
        Value::Float(f) => ("float", float_text(*f)),
        Value::Boolean(b) => ("bool", b.to_string()),
        Value::DateTime(dt) => {
            let kind = match dt.kind {
                DateTimeKind::Offset => "datetime",
                DateTimeKind::Local => "datetime-local",
                DateTimeKind::Date => "date-local",
                DateTimeKind::Time => "time-local",
            };
            (kind, dt.text.clone())
        }
        Value::Array(values) => return Json::Array(values.iter().map(value_json).collect()),
        Value::ArrayOfTables(tables) => {
            return Json::Array(tables.iter().map(table_json).collect());
        }
        Value::Table(table) => return table_json(table),
    };
    json!({ "type": kind, "value": text })
}

fn float_text(f: f64) -> String {
    if f.is_nan() {
        "nan".to_owned()
    } else if f.is_infinite() {
        if f > 0.0 { "inf" } else { "-inf" }.to_owned()
    } else {
        format!("{f:?}")
    }
}
