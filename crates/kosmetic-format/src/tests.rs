use std::path::Path;

use crate::{FormatError, format};

fn fmt(source: &str) -> String {
    format(source).unwrap()
}

#[test]
fn spaces_around_equals_and_dots() {
    assert_eq!(
        fmt("a=1\nb  =  2\nc . d\t=\t3\n"),
        "a = 1\nb = 2\nc.d = 3\n"
    );
}

#[test]
fn headers() {
    assert_eq!(
        fmt("[ a . b ]\nx = 1\n[[ c ]]\ny = 2\n"),
        "[a.b]\nx = 1\n\n[[c]]\ny = 2\n"
    );
}

#[test]
fn single_line_arrays() {
    assert_eq!(
        fmt("a = [1,2 ,3]\nb = [ 1 ]\nc = [ ]\nd = [[1,2],[3]]\n"),
        "a = [1, 2, 3]\nb = [1]\nc = []\nd = [[1, 2], [3]]\n"
    );
}

#[test]
fn inline_tables() {
    assert_eq!(
        fmt("a={b=1,c=2}\nd = {  }\ne = {f = {g=1}}\n"),
        "a = { b = 1, c = 2 }\nd = {}\ne = { f = { g = 1 } }\n"
    );
}

#[test]
fn multi_line_arrays_are_indented_and_get_a_trailing_comma() {
    assert_eq!(
        fmt("a = [\n1,\n    2,\n3\n   ]\n"),
        "a = [\n  1,\n  2,\n  3,\n]\n"
    );
    assert_eq!(
        fmt("a = [\n  \"x\", # first\n  \"y\" # last\n]\n"),
        "a = [\n  \"x\", # first\n  \"y\", # last\n]\n"
    );
}

#[test]
fn nested_multi_line_arrays() {
    assert_eq!(
        fmt("a = [\n[\n1,\n2\n],\n[3]\n]\n"),
        "a = [\n  [\n    1,\n    2,\n  ],\n  [3],\n]\n"
    );
}

#[test]
fn multi_line_array_with_the_closing_bracket_on_the_last_line_keeps_its_shape() {
    assert_eq!(fmt("a = [1, 2,\n  3]\n"), "a = [1, 2,\n  3]\n");
}

#[test]
fn comments_inside_arrays_are_indented() {
    assert_eq!(
        fmt("a = [\n# one\n1,\n  # two\n2,\n]\n"),
        "a = [\n  # one\n  1,\n  # two\n  2,\n]\n"
    );
}

#[test]
fn comments_keep_their_alignment() {
    assert_eq!(
        fmt("a = 1     # one\nbb = 22   # two\nc = 3# three\n"),
        "a = 1     # one\nbb = 22   # two\nc = 3 # three\n"
    );
    assert_eq!(fmt("# c  \t\na = 1 # d   \n"), "# c\na = 1 # d\n");
}

#[test]
fn indentation_and_trailing_whitespace_are_removed() {
    assert_eq!(
        fmt("  [t]  \n    a = 1  \n\t# c\n   \n"),
        "[t]\na = 1\n# c\n"
    );
}

#[test]
fn blank_lines() {
    assert_eq!(
        fmt("\n\na = 1\n\n\n\nb = 2\n[t]\nc = 3\n# about u\n[u]\nd = 4\n\n\n"),
        "a = 1\n\nb = 2\n\n[t]\nc = 3\n\n# about u\n[u]\nd = 4\n"
    );
    assert_eq!(fmt("[t]\n[u]\n"), "[t]\n\n[u]\n");
    assert_eq!(fmt("# top\n\n[t]\n"), "# top\n\n[t]\n");
    assert_eq!(fmt("[t]\n"), "[t]\n");
}

#[test]
fn final_newline() {
    assert_eq!(fmt("a = 1"), "a = 1\n");
    assert_eq!(fmt("a = 1\n\n"), "a = 1\n");
    assert_eq!(fmt(""), "");
    assert_eq!(fmt("\n\n  \n"), "");
}

#[test]
fn line_endings_follow_the_first_one() {
    assert_eq!(fmt("a = 1\nb = 2\r\n\r\n\r\n[t]"), "a = 1\nb = 2\n\n[t]\n");
    assert_eq!(fmt("a=1\r\nb = 2\n"), "a = 1\r\nb = 2\r\n");
    assert_eq!(fmt("a = 1\r\n\r\n\r\nb = 2"), "a = 1\r\n\r\nb = 2\r\n");
}

#[test]
fn strings_and_values_are_untouched() {
    let source = "a = \"\"\"\n  keep  \n\n\n   this\n\"\"\"\nb = 'x  y'\nc = 1979-05-27 07:32:00\nd = 1_000\ne = \"a\\tb\"\n\"q\" = 0xFF\n";
    assert_eq!(fmt(source), source);
}

#[test]
fn quoted_keys_stay_quoted() {
    assert_eq!(fmt("\"a\"=1\n"), "\"a\" = 1\n");
}

#[test]
fn refuses_invalid_documents() {
    assert_eq!(format("a = \n"), Err(FormatError::Invalid));
    assert_eq!(format("a = 1\na = 2\n"), Err(FormatError::Invalid));
}

const SAMPLES: &[&str] = &[
    "a=1",
    "[a]\nb=[1,2,\n3]\n[[c]]\nd={e=1}\n",
    "# lead\n\n\n[t] # trailing\nk = [\n  1, 2, # x\n  3,\n  ]\n\n\n# end",
    "a = [ [ 1 ], [\n2\n] ]\nb = { c = [\n1,\n2\n] }\n",
    "\r\na = 1\r\n\r\n\r\n[t]\r\nb = [\r\n1\r\n]\r\n",
    "x = { }\ny = [ ]\n[ z . w ]\n",
];

fn assert_idempotent(source: &str, label: &str) {
    let once = format(source).unwrap_or_else(|err| panic!("{label}: {err:?}"));
    let twice = format(&once).unwrap_or_else(|err| panic!("{label} (2nd): {err:?}"));
    assert_eq!(once, twice, "{label} is not idempotent");
}

#[test]
fn formatting_is_idempotent() {
    for sample in SAMPLES {
        assert_idempotent(sample, sample);
    }
}

/// Every valid file of the official suite keeps its values and formats to a
/// fixed point. Skipped when the `toml-test` submodule is not checked out.
#[test]
fn toml_test_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../toml-test/tests/valid");
    if !root.is_dir() {
        return;
    }
    let mut stack = vec![root];
    let mut checked = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "toml") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            // Files for TOML 1.1 are not valid for kosmetic.
            if crate::model(&source).is_none() {
                continue;
            }
            assert_idempotent(&source, &path.display().to_string());
            checked += 1;
        }
    }
    assert!(checked > 100, "only {checked} files checked");
}
