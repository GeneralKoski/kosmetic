use std::collections::HashMap;
use std::path::{Path, PathBuf};

use kosmetic_diagnostics::{Diagnostic, Severity, Span};
use kosmetic_rules::{Config, FileKind, fix_source, lint};
use serde_json::{Value, json};

use crate::position::Lines;

const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_REQUEST: i64 = -32600;
const SERVER_NOT_INITIALIZED: i64 = -32002;

/// The config file name, which is also linted as a configuration.
const CONFIG_NAME: &str = ".kosmetic.toml";

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Starting,
    Running,
    ShuttingDown,
}

/// The server: it turns incoming messages into outgoing ones and keeps the
/// text of the open documents. It does no I/O, [`crate::run`] does.
pub struct Server {
    state: State,
    documents: HashMap<String, String>,
    exit_code: Option<i32>,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    /// A server waiting for `initialize`.
    pub fn new() -> Self {
        Self {
            state: State::Starting,
            documents: HashMap::new(),
            exit_code: None,
        }
    }

    /// The process exit code once the client sent `exit`: 0 after a
    /// `shutdown`, 1 without one.
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Handles one incoming message and returns the messages to send back.
    pub fn handle(&mut self, message: &Value) -> Vec<Value> {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Vec::new();
        };
        let id = message.get("id");
        let params = message.get("params").unwrap_or(&Value::Null);

        if method == "exit" {
            self.exit_code = Some(i32::from(self.state != State::ShuttingDown));
            return Vec::new();
        }
        if let Some(id) = id {
            return vec![self.request(method, id, params)];
        }
        match method {
            "textDocument/didOpen" => self.did_open(params),
            "textDocument/didChange" => self.did_change(params),
            "textDocument/didClose" => self.did_close(params),
            // A `.kosmetic.toml` changed: every open document may now have other diagnostics.
            "workspace/didChangeWatchedFiles" => self.publish_all(),
            _ => Vec::new(),
        }
    }

    fn request(&mut self, method: &str, id: &Value, params: &Value) -> Value {
        match (self.state, method) {
            (State::Starting, "initialize") => {
                self.state = State::Running;
                respond(id, &initialize_result())
            }
            (State::Starting, _) => {
                error(id, SERVER_NOT_INITIALIZED, "the server is not initialized")
            }
            (State::ShuttingDown, _) => error(id, INVALID_REQUEST, "the server is shutting down"),
            (State::Running, "shutdown") => {
                self.state = State::ShuttingDown;
                respond(id, &Value::Null)
            }
            (State::Running, "textDocument/formatting") => respond(id, &self.formatting(params)),
            (State::Running, "textDocument/codeAction") => respond(id, &self.code_actions(params)),
            (State::Running, _) => {
                error(id, METHOD_NOT_FOUND, &format!("unknown method `{method}`"))
            }
        }
    }

    fn did_open(&mut self, params: &Value) -> Vec<Value> {
        let document = &params["textDocument"];
        let (Some(uri), Some(text)) = (document["uri"].as_str(), document["text"].as_str()) else {
            return Vec::new();
        };
        self.documents.insert(uri.to_owned(), text.to_owned());
        self.publish(uri)
    }

    fn did_change(&mut self, params: &Value) -> Vec<Value> {
        let Some(uri) = params["textDocument"]["uri"].as_str() else {
            return Vec::new();
        };
        // Full synchronization: the last change holds the whole text.
        let Some(text) = params["contentChanges"]
            .as_array()
            .and_then(|changes| changes.last())
            .and_then(|change| change["text"].as_str())
        else {
            return Vec::new();
        };
        if !self.documents.contains_key(uri) {
            return Vec::new();
        }
        self.documents.insert(uri.to_owned(), text.to_owned());
        self.publish(uri)
    }

    fn did_close(&mut self, params: &Value) -> Vec<Value> {
        let Some(uri) = params["textDocument"]["uri"].as_str() else {
            return Vec::new();
        };
        self.documents.remove(uri);
        vec![notification(
            "textDocument/publishDiagnostics",
            &json!({ "uri": uri, "diagnostics": [] }),
        )]
    }

    fn publish_all(&self) -> Vec<Value> {
        let mut uris: Vec<_> = self.documents.keys().collect();
        uris.sort();
        uris.into_iter().flat_map(|uri| self.publish(uri)).collect()
    }

    fn publish(&self, uri: &str) -> Vec<Value> {
        let Some(text) = self.documents.get(uri) else {
            return Vec::new();
        };
        let diagnostics = diagnostics_of(uri, text);
        let lines = Lines::new(text);
        let diagnostics: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| lsp_diagnostic(uri, &lines, diagnostic))
            .collect();
        vec![notification(
            "textDocument/publishDiagnostics",
            &json!({ "uri": uri, "diagnostics": diagnostics }),
        )]
    }

    fn formatting(&self, params: &Value) -> Value {
        let Some(text) = params["textDocument"]["uri"]
            .as_str()
            .and_then(|uri| self.documents.get(uri))
        else {
            return Value::Null;
        };
        match kosmetic_format::format(text) {
            Ok(formatted) if formatted != *text => {
                json!([whole_document_edit(text, &formatted)])
            }
            Ok(_) => json!([]),
            // Nothing to do for a file with errors: the diagnostics tell why.
            Err(_) => Value::Null,
        }
    }

    fn code_actions(&self, params: &Value) -> Value {
        let Some(uri) = params["textDocument"]["uri"].as_str() else {
            return json!([]);
        };
        let Some(text) = self.documents.get(uri) else {
            return json!([]);
        };
        let only: Vec<&str> = params["context"]["only"]
            .as_array()
            .map(|kinds| kinds.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let wanted = |kind: &str| {
            only.is_empty()
                || only
                    .iter()
                    .any(|only| kind == *only || kind.starts_with(&format!("{only}.")))
        };

        let lines = Lines::new(text);
        let start = lines.offset(
            as_u32(&params["range"]["start"]["line"]),
            as_u32(&params["range"]["start"]["character"]),
        );
        let end = lines.offset(
            as_u32(&params["range"]["end"]["line"]),
            as_u32(&params["range"]["end"]["character"]),
        );

        let mut actions = Vec::new();
        if wanted("quickfix") {
            for diagnostic in diagnostics_of(uri, text) {
                let Some(fix) = &diagnostic.fix else {
                    continue;
                };
                let touches =
                    diagnostic.span.start as usize <= end && start <= diagnostic.span.end as usize;
                if !touches {
                    continue;
                }
                actions.push(json!({
                    "title": format!("Fix: {}", diagnostic.message),
                    "kind": "quickfix",
                    "diagnostics": [lsp_diagnostic(uri, &lines, &diagnostic)],
                    "isPreferred": true,
                    "edit": { "changes": { uri: [text_edit(&lines, fix.span, &fix.replacement)] } },
                }));
            }
        }
        if wanted("source.fixAll") {
            let (config, kind) = config_and_kind(uri);
            if let Ok(fixed) = fix_source(text, &config, kind) {
                if fixed.applied > 0 {
                    actions.push(json!({
                        "title": "Fix all kosmetic problems",
                        "kind": "source.fixAll",
                        "edit": { "changes": { uri: [whole_document_edit(text, &fixed.source)] } },
                    }));
                }
            }
        }
        Value::Array(actions)
    }
}

fn as_u32(value: &Value) -> u32 {
    value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {
            "positionEncoding": "utf-16",
            "textDocumentSync": { "openClose": true, "change": 1 },
            "documentFormattingProvider": true,
            "codeActionProvider": { "codeActionKinds": ["quickfix", "source.fixAll"] },
        },
        "serverInfo": { "name": "kosmetic", "version": env!("CARGO_PKG_VERSION") },
    })
}

fn respond(id: &Value, result: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn notification(method: &str, params: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

fn range(lines: &Lines<'_>, span: Span) -> Value {
    let (start_line, start_character) = lines.position(span.start as usize);
    let (end_line, end_character) = lines.position(span.end as usize);
    json!({
        "start": { "line": start_line, "character": start_character },
        "end": { "line": end_line, "character": end_character },
    })
}

fn text_edit(lines: &Lines<'_>, span: Span, new_text: &str) -> Value {
    json!({ "range": range(lines, span), "newText": new_text })
}

fn whole_document_edit(old: &str, new: &str) -> Value {
    let lines = Lines::new(old);
    let (line, character) = lines.end();
    json!({
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": line, "character": character },
        },
        "newText": new,
    })
}

fn lsp_diagnostic(uri: &str, lines: &Lines<'_>, diagnostic: &Diagnostic) -> Value {
    let severity = match diagnostic.severity {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Info => 3,
    };
    let message = match &diagnostic.help {
        Some(help) => format!("{}\n{help}", diagnostic.message),
        None => diagnostic.message.clone(),
    };
    let related: Vec<_> = diagnostic
        .labels
        .iter()
        .map(|label| {
            json!({
                "location": { "uri": uri, "range": range(lines, label.span) },
                "message": label.message,
            })
        })
        .collect();
    json!({
        "range": range(lines, diagnostic.span),
        "severity": severity,
        "code": diagnostic.code,
        "source": "kosmetic",
        "message": message,
        "relatedInformation": related,
    })
}

/// Decodes `%XX` escapes.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(u8::try_from(high * 16 + low).unwrap_or(b'?'));
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The path of a `file:` URI. Other schemes have no path.
fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    let path = percent_decode(path);
    // `/C:/dir` on Windows.
    let drive = path.as_bytes();
    if drive.len() > 2 && drive[0] == b'/' && drive[2] == b':' && drive[1].is_ascii_alphabetic() {
        return Some(PathBuf::from(&path[1..]));
    }
    path.starts_with('/').then(|| PathBuf::from(path))
}

/// The configuration that applies to a document: the nearest `.kosmetic.toml`
/// in its directory or above, and the kind of the file.
fn config_and_kind(uri: &str) -> (Config, FileKind) {
    let Some(path) = uri_to_path(uri) else {
        return (Config::default(), FileKind::Other);
    };
    let config = path
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .map(|dir| dir.join(CONFIG_NAME))
        .find_map(|candidate| std::fs::read_to_string(candidate).ok())
        .map_or_else(Config::default, |source| Config::from_toml(&source).0);
    (config, FileKind::from_path(&path))
}

/// Everything to report on a document. A `.kosmetic.toml` is also checked as
/// a configuration, so a misspelled rule name shows up while typing it.
fn diagnostics_of(uri: &str, text: &str) -> Vec<Diagnostic> {
    let (config, kind) = config_and_kind(uri);
    let mut diagnostics = lint(text, &config, kind);
    let is_config = uri_to_path(uri)
        .and_then(|path| path.file_name().map(|name| name == CONFIG_NAME))
        .unwrap_or(false);
    if is_config {
        diagnostics.extend(Config::from_toml(text).1.into_iter().filter(|diagnostic| {
            !diagnostic.code.starts_with("syntax/") && !diagnostic.code.starts_with("semantic/")
        }));
        diagnostics.sort_by_key(|diagnostic| diagnostic.span);
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{percent_decode, uri_to_path};

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("a%20b%C3%A9"), "a bé");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
    }

    #[test]
    fn file_uris_become_paths() {
        assert_eq!(
            uri_to_path("file:///home/me/my%20dir/Cargo.toml"),
            Some(PathBuf::from("/home/me/my dir/Cargo.toml"))
        );
        assert_eq!(
            uri_to_path("file://localhost/tmp/a.toml"),
            Some(PathBuf::from("/tmp/a.toml"))
        );
        assert_eq!(
            uri_to_path("file:///C:/work/Cargo.toml"),
            Some(PathBuf::from("C:/work/Cargo.toml"))
        );
        assert_eq!(uri_to_path("untitled:Untitled-1"), None);
        assert_eq!(uri_to_path("http://example.com/a.toml"), None);
    }
}
