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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
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

#[test]
fn errors_fail_the_run_and_show_labels() {
    let output = kosmetic(&["check", "--color", "never", "invalid.toml"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout.contains("invalid.toml:2:16: error[syntax/unclosed-delimiter]"));
    assert!(stdout.contains("invalid.toml:3:1: error[semantic/duplicate-key]"));
    assert!(stdout.contains("| ---- first defined here"));
}

#[test]
fn color_can_be_forced() {
    let output = kosmetic(&["check", "--color", "always", "invalid.toml"]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("\x1b[1;31merror["));
}

#[test]
fn config_is_discovered_from_the_working_directory() {
    let output = Command::new(env!("CARGO_BIN_EXE_kosmetic"))
        .args(["check", "--color", "never", "unsorted.toml"])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/configured"))
        .output()
        .expect("failed to run kosmetic");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("error[style/sorted-keys]"));
    assert!(!stdout.contains("no-trailing-whitespace"));
}

#[test]
fn invalid_config_stops_the_run() {
    let output = kosmetic(&[
        "check",
        "--color",
        "never",
        "--config",
        "bad-config.toml",
        "clean.toml",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stdout
            .contains("bad-config.toml:2:1: error[schema/unknown-key]: unknown rule `style/nope`")
    );
}
