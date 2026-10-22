use kosmetic_diagnostics::Diagnostic;

use crate::testing::lint_file;
use crate::{FileKind, Level};

fn cargo(code: &str, source: &str) -> Vec<Diagnostic> {
    lint_file(code, source, None, FileKind::CargoManifest)
}

fn pyproject(code: &str, source: &str) -> Vec<Diagnostic> {
    lint_file(code, source, None, FileKind::PyProject)
}

fn messages(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(|d| d.message.as_str()).collect()
}

/// A manifest that every Cargo rule accepts.
const GOOD_CARGO: &str = r#"
[package]
name = "demo"
version = "0.1.0"
edition = "2021"
rust-version = "1.85"
description = "x"
license = "MIT"
keywords = ["a", "b"]

[lib]
name = "demo"
path = "src/lib.rs"

[[bin]]
name = "demo-cli"

[features]
default = ["extra"]
extra = []

[dependencies]
serde = { version = "1", features = ["derive"] }
local = { path = "../local" }
plain = "^1.2"
dotted.version = "0.4"

[dependencies.other]
git = "https://example.com/other"
branch = "main"

[dev-dependencies]
proptest = ">=1.0, <2"

[target.'cfg(windows)'.dependencies]
winapi = "0.3"

[profile.release]
lto = "thin"
"#;

const ALL_CARGO: &[&str] = &[
    "schema/cargo-unknown-key",
    "schema/cargo-missing-field",
    "schema/cargo-invalid-value",
    "schema/cargo-wildcard-dependency",
    "schema/cargo-publish-metadata",
];

const ALL_PYPROJECT: &[&str] = &[
    "schema/pyproject-unknown-key",
    "schema/pyproject-missing-field",
    "schema/pyproject-invalid-value",
];

const GOOD_PYPROJECT: &str = r#"
[build-system]
requires = ["setuptools>=61", "wheel"]
build-backend = "setuptools.build_meta"

[project]
name = "my-package"
version = "1.2.3rc1"
description = "x"
readme = "README.md"
requires-python = ">=3.9,<4"
license = { text = "MIT" }
authors = [{ name = "A", email = "a@example.com" }]
keywords = ["a"]
classifiers = ["Programming Language :: Python :: 3"]
dependencies = ["requests[socks]>=2.0; python_version < '3.13'", "click"]

[project.optional-dependencies]
dev = ["pytest>=7"]

[project.urls]
Homepage = "https://example.com"

[project.scripts]
tool = "my_package:main"

[project.entry-points."my.plugins"]
a = "my_package.plugin:A"

[dependency-groups]
test = ["pytest", { include-group = "lint" }]
lint = ["ruff"]

[tool.ruff]
line-length = 100
"#;

#[test]
fn good_files_pass_every_rule() {
    for code in ALL_CARGO {
        let level = Some(Level::Warn);
        assert_eq!(
            lint_file(code, GOOD_CARGO, level, FileKind::CargoManifest),
            Vec::new(),
            "{code}"
        );
    }
    for code in ALL_PYPROJECT {
        assert_eq!(pyproject(code, GOOD_PYPROJECT), Vec::new(), "{code}");
    }
}

#[test]
fn schema_rules_only_run_on_their_file() {
    let source = "[package]\nedtion = \"2021\"\n";
    for code in ALL_CARGO.iter().chain(ALL_PYPROJECT) {
        for kind in [FileKind::Other, FileKind::PyProject] {
            if code.contains("cargo") || kind == FileKind::Other {
                assert_eq!(lint_file(code, source, Some(Level::Warn), kind), Vec::new());
            }
        }
    }
}

#[test]
fn cargo_unknown_key() {
    let code = "schema/cargo-unknown-key";
    let source = "[package]\nname = \"a\"\nedtion = \"2021\"\nzzz = 1\n[dependencies]\nserde = { verison = \"1\" }\n[profile.release]\nlpt = 1\n";
    let diagnostics = cargo(code, source);
    assert_eq!(
        messages(&diagnostics),
        [
            "unknown key `edtion` in `[package]`",
            "unknown key `zzz` in `[package]`",
            "unknown key `verison` in dependency `serde`",
            "unknown key `lpt` in `[profile.release]`",
        ]
    );
    assert_eq!(
        diagnostics[0].help.as_deref(),
        Some("did you mean `edition`?")
    );
    assert_eq!(
        diagnostics[1].help.as_deref(),
        Some("check the spelling of the key")
    );
    assert_eq!(&source[diagnostics[0].span.range()], "edtion");
    assert_eq!(
        messages(&cargo(
            code,
            "pakage = 1\n[lib]\nnme = 1\n[[bin]]\nnme = 1\n"
        )),
        [
            "unknown key `pakage` in the manifest",
            "unknown key `nme` in `[lib]`",
            "unknown key `nme` in `[[bin]]`",
        ]
    );
    assert_eq!(
        lint_file(code, source, Some(Level::Off), FileKind::CargoManifest),
        Vec::new()
    );
}

#[test]
fn cargo_missing_field() {
    let code = "schema/cargo-missing-field";
    assert_eq!(
        messages(&cargo(code, "")),
        ["the manifest has neither `[package]` nor `[workspace]`"]
    );
    assert_eq!(cargo(code, "[workspace]\nmembers = [\"a\"]\n"), Vec::new());

    let source = "[package]\nversion = \"1\"\n[[bin]]\npath = \"a.rs\"\n[[bin]]\nname = \"b\"\n[[example]]\npath = \"e.rs\"\n[dependencies]\nnosource = { features = [\"a\"] }\nok = { path = \"x\" }\ninherited.workspace = true\n";
    let diagnostics = cargo(code, source);
    assert_eq!(
        messages(&diagnostics),
        [
            "`[package]` is missing `name`",
            "`[[bin]]` is missing `name`",
            "`[[example]]` is missing `name`",
            "dependency `nosource` must specify one of `version`, `path`, `git` or `workspace`",
        ]
    );
    // Each target points at its own header.
    assert_eq!(&source[diagnostics[1].span.range()], "bin");
    assert_eq!(
        diagnostics[1].span.start as usize,
        source.find("bin").unwrap()
    );
    assert_eq!(
        lint_file(code, source, Some(Level::Off), FileKind::CargoManifest),
        Vec::new()
    );
}

#[test]
fn cargo_missing_name_points_at_the_right_bin() {
    let source = "[package]\nname = \"a\"\n[[bin]]\nname = \"one\"\n[[bin]]\npath = \"two.rs\"\n";
    let diagnostics = cargo("schema/cargo-missing-field", source);
    assert_eq!(diagnostics.len(), 1);
    let second = source.rfind("bin").unwrap();
    assert_eq!(diagnostics[0].span.start as usize, second);
}

#[test]
fn cargo_invalid_value() {
    let code = "schema/cargo-invalid-value";
    let source = "[package]\nname = \"1bad\"\nversion = \"1.0\"\nedition = 2021\nrust-version = \"1\"\nresolver = \"9\"\nkeywords = \"a\"\nauthors.workspace = true\n";
    let diagnostics = cargo(code, source);
    assert_eq!(
        messages(&diagnostics),
        [
            "`1bad` is not a valid package name",
            "`1.0` is not a valid semantic version",
            "`package.edition` must be a string, found an integer",
            "`1` is not a valid `rust-version`",
            "unknown resolver `9`",
            "`package.keywords` must be an array of strings, found a string",
        ]
    );
    assert_eq!(
        diagnostics[2].help.as_deref(),
        Some("quote the value: \"2021\"")
    );
    assert_eq!(&source[diagnostics[1].span.range()], "\"1.0\"");

    assert_eq!(
        messages(&cargo(
            code,
            "[package]\nname = \"a\"\nedition = \"2022\"\n[lib]\nedition = \"1999\"\ntest = \"yes\"\n"
        )),
        [
            "unknown edition `2022`",
            "unknown edition `1999`",
            "`lib.test` must be a boolean, found a string",
        ]
    );
    assert_eq!(
        messages(&cargo(
            code,
            "[features]\nx = \"a\"\n[workspace]\nmembers = \"a\"\n"
        )),
        [
            "`features.x` must be an array of strings, found a string",
            "`workspace.members` must be an array of strings, found a string",
        ]
    );
    assert_eq!(
        lint_file(code, source, Some(Level::Off), FileKind::CargoManifest),
        Vec::new()
    );
}

#[test]
fn cargo_invalid_dependencies() {
    let code = "schema/cargo-invalid-value";
    let source = r#"
[dependencies]
a = "not a version"
b = { version = "1.x.3" }
c = { git = "https://x", path = "../c" }
d = { path = "../d", branch = "main" }
e = { git = "https://x", branch = "a", tag = "b" }
f = { workspace = true, version = "1" }
g = true
h = { version = "1", optional = "yes" }
i = { version = "1", features = ["a"], optional = true, default-features = false }
"#;
    assert_eq!(
        messages(&cargo(code, source)),
        [
            "invalid version requirement `not a version` for dependency `a`",
            "invalid version requirement `1.x.3` for dependency `b`",
            "dependency `c` cannot have both `git` and `path`",
            "`branch` of dependency `d` requires `git`",
            "dependency `e` can have only one of `branch`, `tag` and `rev`",
            "`workspace = true` of dependency `f` cannot be combined with `version`",
            "dependency `g` must be a version string or a table, found a boolean",
            "`h.optional` must be a boolean, found a string",
        ]
    );
}

#[test]
fn cargo_workspace_package_and_inheritance() {
    let code = "schema/cargo-invalid-value";
    let source = "[workspace.package]\nversion = \"x\"\nedition = \"2021\"\n[package]\nname = \"a\"\nversion.workspace = true\nedition = { workspace = true }\n";
    assert_eq!(
        messages(&cargo(code, source)),
        ["`x` is not a valid semantic version"]
    );
}

#[test]
fn cargo_wildcard_dependency() {
    let code = "schema/cargo-wildcard-dependency";
    let source = "[dependencies]\na = \"*\"\nb = { version = \"*\" }\nc = \"1\"\nd = \"1.*\"\ne = { path = \"x\" }\n[workspace.dependencies]\nf = \"*\"\n";
    let diagnostics = cargo(code, source);
    assert_eq!(
        messages(&diagnostics),
        [
            "dependency `a` accepts any version",
            "dependency `b` accepts any version",
            "dependency `f` accepts any version",
        ]
    );
    assert_eq!(&source[diagnostics[0].span.range()], "\"*\"");
    assert_eq!(
        lint_file(code, source, Some(Level::Off), FileKind::CargoManifest),
        Vec::new()
    );
}

#[test]
fn cargo_publish_metadata() {
    let code = "schema/cargo-publish-metadata";
    let source = "[package]\nname = \"a\"\nkeywords = [\"1\", \"2\", \"3\", \"4\", \"5\", \"6\", \"has space\"]\ncategories = [\"a\",\"b\",\"c\",\"d\",\"e\",\"f\"]\n";
    let warn = |source: &str| lint_file(code, source, Some(Level::Warn), FileKind::CargoManifest);
    assert_eq!(
        messages(&warn(source)),
        [
            "`[package]` has no `description`, crates.io requires it",
            "`[package]` has no `license` or `license-file`, crates.io requires one",
            "crates.io accepts at most 5 `keywords`, found 7",
            "`has space` is not a valid crates.io keyword",
            "crates.io accepts at most 5 `categories`, found 6",
        ]
    );
    assert_eq!(
        warn("[package]\nname = \"a\"\npublish = false\n"),
        Vec::new()
    );
    assert_eq!(
        warn("[package]\nname = \"a\"\ndescription.workspace = true\nlicense-file = \"L\"\n"),
        Vec::new()
    );
    assert_eq!(cargo(code, source), Vec::new(), "off by default");
}

#[test]
fn pyproject_unknown_key() {
    let code = "schema/pyproject-unknown-key";
    let source = "[tools]\nx = 1\n[project]\nnme = \"a\"\nversion = \"1\"\n[build-system]\nrequires = []\nbackend = \"x\"\n";
    let diagnostics = pyproject(code, source);
    assert_eq!(
        messages(&diagnostics),
        [
            "unknown key `tools` in the file",
            "unknown key `nme` in `[project]`",
            "unknown key `backend` in `[build-system]`",
        ]
    );
    assert_eq!(diagnostics[0].help.as_deref(), Some("did you mean `tool`?"));
    assert_eq!(diagnostics[1].help.as_deref(), Some("did you mean `name`?"));
    assert_eq!(
        lint_file(code, source, Some(Level::Off), FileKind::PyProject),
        Vec::new()
    );
}

#[test]
fn pyproject_missing_field() {
    let code = "schema/pyproject-missing-field";
    assert_eq!(
        messages(&pyproject(
            code,
            "[project]\ndescription = \"x\"\n[build-system]\nbuild-backend = \"b\"\n"
        )),
        [
            "`[project]` is missing `name`",
            "`[project]` is missing `version`",
            "`[build-system]` is missing `requires`",
        ]
    );
    assert_eq!(
        pyproject(code, "[project]\nname = \"a\"\ndynamic = [\"version\"]\n"),
        Vec::new()
    );
    assert_eq!(pyproject(code, "[tool.x]\na = 1\n"), Vec::new());
    assert_eq!(
        lint_file(code, "[project]\n", Some(Level::Off), FileKind::PyProject),
        Vec::new()
    );
}

#[test]
fn pyproject_invalid_value() {
    let code = "schema/pyproject-invalid-value";
    let source = r#"
[project]
name = "-bad"
version = "1.0.x"
requires-python = "3.8"
dependencies = ["requests>=", "click", "ok>=1", 5]
keywords = "a"
authors = [{ name = "A", mail = "x" }]
readme = 3
"#;
    let diagnostics = pyproject(code, source);
    assert_eq!(
        messages(&diagnostics),
        [
            "`-bad` is not a valid project name",
            "`1.0.x` is not a valid PEP 440 version",
            "`3.8` is not a valid version specifier",
            "`project.dependencies` must be an array of strings, found an array with items that are not strings",
            "invalid requirement `requests>=`: invalid version specifier",
            "`project.keywords` must be an array of strings, found a string",
            "unknown key `mail` in an entry of `project.authors`",
            "`project.readme` must be a string or a table, found an integer",
        ]
    );
    assert!(diagnostics.iter().all(|d| d.code == code));
    let requirement = diagnostics
        .iter()
        .find(|d| d.message.starts_with("invalid requirement"))
        .unwrap();
    assert_eq!(&source[requirement.span.range()], "\"requests>=\"");
}

#[test]
fn pyproject_authors_shape() {
    let code = "schema/pyproject-invalid-value";
    assert_eq!(
        messages(&pyproject(
            code,
            "[project]\nname = \"a\"\nauthors = \"me\"\nmaintainers = [1]\n"
        )),
        [
            "`project.authors` must be an array of tables with `name` and `email`",
            "`project.maintainers` must be an array of tables with `name` and `email`",
        ]
    );
    assert_eq!(
        pyproject(
            code,
            "[project]\nname = \"a\"\n[[project.authors]]\nname = \"A\"\n"
        ),
        Vec::new()
    );
}

#[test]
fn pyproject_dynamic() {
    let code = "schema/pyproject-invalid-value";
    let source = "[project]\nname = \"a\"\nversion = \"1\"\ndynamic = [\"version\", \"name\", \"bogus\", \"readme\"]\n";
    let diagnostics = pyproject(code, source);
    assert_eq!(
        messages(&diagnostics),
        [
            "`version` is listed in `dynamic` but also set",
            "`name` cannot be dynamic",
            "`bogus` is not a field that can be dynamic",
        ]
    );
    assert_eq!(diagnostics[0].labels.len(), 1);
}

#[test]
fn poetry_may_extend_dynamic_fields() {
    let code = "schema/pyproject-invalid-value";
    let source = "[project]\nname = \"a\"\nversion = \"1\"\nclassifiers = [\"x\"]\ndynamic = [\"classifiers\"]\n[build-system]\nrequires = [\"poetry-core\"]\nbuild-backend = \"poetry.core.masonry.api\"\n";
    assert_eq!(pyproject(code, source), Vec::new());
    assert_eq!(
        pyproject(
            code,
            &source.replace("poetry.core.masonry.api", "setuptools.build_meta")
        )
        .len(),
        1
    );
}

#[test]
fn pyproject_optional_dependencies_and_groups() {
    let code = "schema/pyproject-invalid-value";
    let source = r#"
[project]
name = "a"
version = "1"

[project.optional-dependencies]
dev = ["pytest>="]
"bad extra" = ["x"]
wrong = "x"

[project.entry-points.console_scripts]
a = "b:c"

[project.scripts]
x = 1

[build-system]
requires = ["setuptools>=", 3]
build-backend = ""

[dependency-groups]
g = ["ok", { include-group = "h" }, { other = 1 }, 7, "bad>="]
h = "x"
"#;
    assert_eq!(
        messages(&pyproject(code, source)),
        [
            "invalid requirement `pytest>=`: invalid version specifier",
            "`bad extra` is not a valid extra name",
            "`project.optional-dependencies.wrong` must be an array of strings, found a string",
            "`console_scripts` cannot be an entry point group",
            "`project.scripts.x` must be a string, found an integer",
            "`build-system.requires` must be an array of strings, found an array with items that are not strings",
            "invalid requirement `setuptools>=`: invalid version specifier",
            "`build-system.build-backend` is empty",
            "an item of `dependency-groups.g` must be a requirement or `{ include-group = \"...\" }`, found a table",
            "an item of `dependency-groups.g` must be a requirement or `{ include-group = \"...\" }`, found an integer",
            "invalid requirement `bad>=`: invalid version specifier",
            "`dependency-groups.h` must be an array of strings, found a string",
        ]
    );
}
