//! Protocol tests: messages in, messages out, no I/O.

use std::path::Path;

use kosmetic_lsp::{Server, read_message, run, write_message};
use serde_json::{Value, json};

fn request(id: i64, method: &str, params: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn notification(method: &str, params: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

fn started() -> Server {
    let mut server = Server::new();
    let replies = server.handle(&request(1, "initialize", &json!({ "capabilities": {} })));
    assert_eq!(replies.len(), 1);
    server
}

fn open(server: &mut Server, uri: &str, text: &str) -> Vec<Value> {
    server.handle(&notification(
        "textDocument/didOpen",
        &json!({ "textDocument": { "uri": uri, "languageId": "toml", "version": 1, "text": text } }),
    ))
}

fn diagnostics(replies: &[Value]) -> &Vec<Value> {
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert_eq!(replies[0]["method"], "textDocument/publishDiagnostics");
    replies[0]["params"]["diagnostics"].as_array().unwrap()
}

#[test]
fn initialize_announces_the_capabilities() {
    let mut server = Server::new();
    let replies = server.handle(&request(7, "initialize", &json!({})));
    let result = &replies[0]["result"];
    assert_eq!(replies[0]["id"], 7);
    assert_eq!(result["capabilities"]["textDocumentSync"]["change"], 1);
    assert_eq!(result["capabilities"]["documentFormattingProvider"], true);
    assert_eq!(result["capabilities"]["positionEncoding"], "utf-16");
    assert_eq!(result["serverInfo"]["name"], "kosmetic");
}

#[test]
fn requests_before_initialize_are_refused() {
    let mut server = Server::new();
    let replies = server.handle(&request(1, "textDocument/formatting", &json!({})));
    assert_eq!(replies[0]["error"]["code"], -32002);
}

#[test]
fn unknown_requests_get_an_error_and_unknown_notifications_are_ignored() {
    let mut server = started();
    let replies = server.handle(&request(2, "workspace/whatever", &json!({})));
    assert_eq!(replies[0]["error"]["code"], -32601);
    assert!(
        server
            .handle(&notification("$/cancelRequest", &json!({ "id": 2 })))
            .is_empty()
    );
    assert!(server.handle(&Value::Null).is_empty());
    assert!(
        server
            .handle(&json!({ "jsonrpc": "2.0", "id": 3, "result": null }))
            .is_empty()
    );
}

#[test]
fn opening_a_document_publishes_diagnostics() {
    let mut server = started();
    let replies = open(&mut server, "file:///tmp/none/a.toml", "a = 1  \nb =\n");
    let published = diagnostics(&replies);
    assert_eq!(replies[0]["params"]["uri"], "file:///tmp/none/a.toml");
    let codes: Vec<_> = published
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert_eq!(
        codes,
        ["style/no-trailing-whitespace", "syntax/expected-value"]
    );

    let first = &published[0];
    assert_eq!(first["severity"], 2);
    assert_eq!(first["source"], "kosmetic");
    assert_eq!(
        first["range"]["start"],
        json!({ "line": 0, "character": 5 })
    );
    assert_eq!(first["range"]["end"], json!({ "line": 0, "character": 7 }));
    assert!(
        first["message"]
            .as_str()
            .unwrap()
            .contains("remove the spaces")
    );
    assert_eq!(published[1]["severity"], 1);
}

#[test]
fn columns_are_utf16_units() {
    let mut server = started();
    let replies = open(&mut server, "file:///tmp/none/a.toml", "a = \"😀\"  \n");
    let range = &diagnostics(&replies)[0]["range"];
    assert_eq!(range["start"]["character"], 8);
    assert_eq!(range["end"]["character"], 10);
}

#[test]
fn changes_republish_and_close_clears() {
    let mut server = started();
    let uri = "file:///tmp/none/a.toml";
    assert_eq!(diagnostics(&open(&mut server, uri, "a = 1 \n")).len(), 1);
    let replies = server.handle(&notification(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": "a = 1\n" }],
        }),
    ));
    assert!(diagnostics(&replies).is_empty());

    let replies = server.handle(&notification(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": uri } }),
    ));
    assert!(diagnostics(&replies).is_empty());
    assert!(
        server.handle(&request(
            3,
            "textDocument/formatting",
            &json!({ "textDocument": { "uri": uri } })
        ))[0]["result"]
            .is_null()
    );
}

#[test]
fn related_information_comes_from_labels() {
    let mut server = started();
    let replies = open(&mut server, "file:///tmp/none/a.toml", "a = 1\na = 2\n");
    let duplicate = &diagnostics(&replies)[0];
    assert_eq!(duplicate["code"], "semantic/duplicate-key");
    let related = &duplicate["relatedInformation"][0];
    assert_eq!(related["message"], "first defined here");
    assert_eq!(related["location"]["range"]["start"]["line"], 0);
}

#[test]
fn schema_rules_follow_the_file_name() {
    let mut server = started();
    let replies = open(
        &mut server,
        "file:///tmp/none/Cargo.toml",
        "[package]\nname = \"a\"\nedtion = \"2021\"\n",
    );
    let published = diagnostics(&replies);
    assert_eq!(published[0]["code"], "schema/cargo-unknown-key");
    assert!(
        published[0]["message"]
            .as_str()
            .unwrap()
            .contains("did you mean `edition`?")
    );

    let replies = open(
        &mut server,
        "file:///tmp/none/other.toml",
        "[package]\nname = \"a\"\nedtion = \"2021\"\n",
    );
    assert!(diagnostics(&replies).is_empty());
}

#[test]
fn formatting_returns_one_edit_for_the_whole_document() {
    let mut server = started();
    let uri = "file:///tmp/none/a.toml";
    open(&mut server, uri, "a=[1,2]\n\n\nb = 2");
    let reply = &server.handle(&request(
        4,
        "textDocument/formatting",
        &json!({ "textDocument": { "uri": uri }, "options": { "tabSize": 2 } }),
    ))[0];
    let edits = reply["result"].as_array().unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["newText"], "a = [1, 2]\n\nb = 2\n");
    assert_eq!(
        edits[0]["range"]["start"],
        json!({ "line": 0, "character": 0 })
    );
    assert_eq!(
        edits[0]["range"]["end"],
        json!({ "line": 3, "character": 5 })
    );
}

#[test]
fn formatting_a_formatted_or_invalid_document_returns_nothing() {
    let mut server = started();
    let uri = "file:///tmp/none/a.toml";
    open(&mut server, uri, "a = 1\n");
    let params = json!({ "textDocument": { "uri": uri } });
    assert_eq!(
        server.handle(&request(5, "textDocument/formatting", &params))[0]["result"],
        json!([])
    );

    open(&mut server, uri, "a = \n");
    assert!(server.handle(&request(6, "textDocument/formatting", &params))[0]["result"].is_null());
}

fn code_actions(server: &mut Server, uri: &str, range: &Value, only: Option<&Value>) -> Vec<Value> {
    let mut context = json!({ "diagnostics": [] });
    if let Some(only) = only {
        context["only"] = only.clone();
    }
    let reply = &server.handle(&request(
        9,
        "textDocument/codeAction",
        &json!({ "textDocument": { "uri": uri }, "range": range, "context": context }),
    ))[0];
    reply["result"].as_array().unwrap().clone()
}

#[test]
fn quick_fixes_cover_the_diagnostics_in_range() {
    let mut server = started();
    let uri = "file:///tmp/none/a.toml";
    open(&mut server, uri, "a = 1  \nb=2\n");
    let on_first_line =
        json!({ "start": { "line": 0, "character": 6 }, "end": { "line": 0, "character": 6 } });
    let actions = code_actions(&mut server, uri, &on_first_line, Some(&json!(["quickfix"])));
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0]["kind"], "quickfix");
    assert_eq!(actions[0]["title"], "Fix: trailing whitespace");
    let edit = &actions[0]["edit"]["changes"][uri][0];
    assert_eq!(edit["newText"], "");
    assert_eq!(edit["range"]["start"], json!({ "line": 0, "character": 5 }));
    assert_eq!(edit["range"]["end"], json!({ "line": 0, "character": 7 }));
    assert_eq!(
        actions[0]["diagnostics"][0]["code"],
        "style/no-trailing-whitespace"
    );

    let on_second_line =
        json!({ "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 3 } });
    let actions = code_actions(&mut server, uri, &on_second_line, None);
    let titles: Vec<_> = actions
        .iter()
        .map(|a| a["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        [
            "Fix: expected one space on each side of `=`",
            "Fix all kosmetic problems"
        ]
    );
}

#[test]
fn fix_all_rewrites_the_document() {
    let mut server = started();
    let uri = "file:///tmp/none/a.toml";
    open(&mut server, uri, "a=1  \n#c\n");
    let nowhere =
        json!({ "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } });
    let actions = code_actions(&mut server, uri, &nowhere, Some(&json!(["source"])));
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0]["kind"], "source.fixAll");
    assert_eq!(
        actions[0]["edit"]["changes"][uri][0]["newText"],
        "a = 1\n# c\n"
    );

    open(&mut server, uri, "a = 1\n");
    assert!(code_actions(&mut server, uri, &nowhere, None).is_empty());
    open(&mut server, uri, "a = \n");
    assert!(code_actions(&mut server, uri, &nowhere, None).is_empty());
}

#[test]
fn the_nearest_config_file_decides_the_levels() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp-config");
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    std::fs::write(
        dir.join(".kosmetic.toml"),
        "[rules]\n\"style/no-trailing-whitespace\" = \"error\"\n\"style/final-newline\" = \"off\"\n",
    )
    .unwrap();
    let uri = format!("file://{}/nested/a.toml", dir.display());

    let mut server = started();
    let replies = open(&mut server, &uri, "a = 1 ");
    let published = diagnostics(&replies);
    assert_eq!(published.len(), 1);
    assert_eq!(published[0]["code"], "style/no-trailing-whitespace");
    assert_eq!(published[0]["severity"], 1);
}

#[test]
fn changing_the_config_republishes_every_open_document() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp-config-change");
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join(".kosmetic.toml");
    std::fs::write(&config, "[rules]\n").unwrap();
    let uri = format!("file://{}/a.toml", dir.display());

    let mut server = started();
    let replies = open(&mut server, &uri, "a = 1 \n");
    assert_eq!(diagnostics(&replies)[0]["severity"], 2);

    std::fs::write(
        &config,
        "[rules]\n\"style/no-trailing-whitespace\" = \"error\"\n",
    )
    .unwrap();
    let replies = server.handle(&notification(
        "workspace/didChangeWatchedFiles",
        &json!({ "changes": [{ "uri": format!("file://{}", config.display()), "type": 2 }] }),
    ));
    assert_eq!(replies[0]["params"]["uri"], uri.as_str());
    assert_eq!(diagnostics(&replies)[0]["severity"], 1);
}

#[test]
fn a_config_file_is_checked_as_a_config() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp-config-file");
    std::fs::create_dir_all(&dir).unwrap();
    let uri = format!("file://{}/.kosmetic.toml", dir.display());

    let mut server = started();
    let replies = open(&mut server, &uri, "[rules]\n\"style/nope\" = \"warn\"\n");
    let published = diagnostics(&replies);
    assert_eq!(published.len(), 1);
    assert_eq!(published[0]["code"], "schema/unknown-key");
    assert!(
        published[0]["message"]
            .as_str()
            .unwrap()
            .contains("unknown rule `style/nope`")
    );
}

#[test]
fn shutdown_and_exit() {
    let mut server = started();
    assert_eq!(server.exit_code(), None);
    let replies = server.handle(&request(8, "shutdown", &Value::Null));
    assert!(replies[0]["result"].is_null());
    assert_eq!(
        server.handle(&request(9, "textDocument/formatting", &json!({})))[0]["error"]["code"],
        -32600
    );
    server.handle(&notification("exit", &Value::Null));
    assert_eq!(server.exit_code(), Some(0));

    let mut abrupt = started();
    abrupt.handle(&notification("exit", &Value::Null));
    assert_eq!(abrupt.exit_code(), Some(1));
}

#[test]
fn a_whole_session_over_framed_streams() {
    let mut input = Vec::new();
    for message in [
        request(1, "initialize", &json!({})),
        notification("initialized", &json!({})),
        notification(
            "textDocument/didOpen",
            &json!({ "textDocument": { "uri": "file:///tmp/none/a.toml", "text": "a = 1 \n" } }),
        ),
        request(2, "shutdown", &Value::Null),
        notification("exit", &Value::Null),
    ] {
        write_message(&mut input, &message).unwrap();
    }
    let mut output = Vec::new();
    let code = run(std::io::Cursor::new(input), &mut output).unwrap();
    assert_eq!(code, 0);

    let mut reader = std::io::Cursor::new(output);
    let mut methods = Vec::new();
    while let Some(message) = read_message(&mut reader).unwrap() {
        methods.push(
            message["method"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| message["id"].as_i64().map(|id| id.to_string())),
        );
    }
    assert_eq!(
        methods,
        [
            Some("1".to_owned()),
            Some("textDocument/publishDiagnostics".to_owned()),
            Some("2".to_owned())
        ]
    );
}

#[test]
fn the_input_ending_without_exit_is_a_failure() {
    let code = run(std::io::Cursor::new(Vec::new()), Vec::new()).unwrap();
    assert_eq!(code, 1);
}
