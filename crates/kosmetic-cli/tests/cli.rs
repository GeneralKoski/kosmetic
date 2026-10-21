//! End-to-end tests: run the real binary on the files in `tests/fixtures`.

use std::path::Path;
use std::process::Command;

fn kosmetic(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_kosmetic"))
        .args(args)
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"))
        .output()
        .expect("failed to run kosmetic")
}

#[test]
fn clean_file_passes() {
    let output = kosmetic(&["check", "clean.toml"]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn warnings_are_reported_but_do_not_fail() {
    let output = kosmetic(&["check", "trailing-whitespace.toml"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("trailing-whitespace.toml:2:18: warning[style/no-trailing-whitespace]")
    );
}

#[test]
fn json_output_is_valid() {
    let output = kosmetic(&["check", "--format", "json", "trailing-whitespace.toml"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim_start().starts_with('['));
    assert!(stdout.contains("\"code\": \"style/no-trailing-whitespace\""));
}

#[test]
fn missing_file_exits_with_2() {
    let output = kosmetic(&["check", "does-not-exist.toml"]);
    assert_eq!(output.status.code(), Some(2));
}
