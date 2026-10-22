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

/// Writes `content` to a fresh file in the target directory and returns its path.
fn temp_file(name: &str, content: &str) -> std::path::PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("input.toml");
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn fix_rewrites_the_file() {
    let path = temp_file("fix-rewrites", "name=\"x\"  \n#c\n\n\n[t]\n\"a\" = 1");
    let output = kosmetic(&["check", "--fix", "--color", "never", path.to_str().unwrap()]);
    assert!(output.status.success());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "name = \"x\"\n# c\n\n[t]\na = 1\n"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("fixed 6 problems"));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
}

#[test]
fn fix_reports_what_it_cannot_fix() {
    let path = temp_file("fix-remaining", "b = 1\na = 2\n");
    let config =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/configured/.kosmetic.toml");
    let output = kosmetic(&[
        "check",
        "--fix",
        "--color",
        "never",
        "--config",
        config.to_str().unwrap(),
        path.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("style/sorted-keys"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "b = 1\na = 2\n");
}

#[test]
fn fix_does_not_touch_files_with_errors() {
    let source = "a=1  \nb = \n";
    let path = temp_file("fix-invalid", source);
    let output = kosmetic(&["check", "--fix", "--color", "never", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("has errors, not fixing"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
}

#[test]
fn fmt_rewrites_the_file() {
    let path = temp_file("fmt-rewrites", "a=[1,2]\n\n\n[ t ]\nb = {c=1}");
    let output = kosmetic(&["fmt", path.to_str().unwrap()]);
    assert!(output.status.success());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "a = [1, 2]\n\n[t]\nb = { c = 1 }\n"
    );
}

#[test]
fn fmt_check_does_not_write() {
    let source = "a=1\n";
    let path = temp_file("fmt-check", source);
    let output = kosmetic(&["fmt", "--check", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("would reformat"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);

    let formatted = temp_file("fmt-check-clean", "a = 1\n");
    let output = kosmetic(&["fmt", "--check", formatted.to_str().unwrap()]);
    assert!(output.status.success());
}

#[test]
fn fmt_refuses_files_with_errors() {
    let source = "a = \n";
    let path = temp_file("fmt-invalid", source);
    let output = kosmetic(&["fmt", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("has errors, not formatting"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
}

#[test]
fn fmt_reads_standard_input() {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(env!("CARGO_BIN_EXE_kosmetic"))
        .args(["fmt", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"a=1").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "a = 1\n");
}

#[test]
fn formatted_output_passes_check() {
    let path = temp_file("fmt-then-check", "#x\na=[\n1,\n 2\n]\n[ t ]\n\"k\"=1  ");
    assert!(kosmetic(&["fmt", path.to_str().unwrap()]).status.success());
    let output = kosmetic(&["check", "--color", "never", path.to_str().unwrap()]);
    assert!(output.status.success());
    // The formatter does not touch comment text, `check` still asks for the space.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("no-trailing-whitespace"));
    assert!(!stdout.contains("no-multiple-blank-lines"));
    assert!(!stdout.contains("spaces-around-equals"));
    assert!(!stdout.contains("final-newline"));
}
