# kosmetic

A fast TOML linter written in Rust, with a hand-written lexer and a lossless parser.

```text
$ kosmetic check Cargo.toml
Cargo.toml:2:18: warning[style/no-trailing-whitespace]: trailing whitespace
  2 | name = "kosmetic"
    |                  ^^^
  = help: remove the spaces at the end of the line
```

> Status: early development. The lexer, the lossless parser and the value
> model pass the `toml-test` suite, diagnostics show labels and colors,
> seventeen configurable rules run, `--fix` and `kosmetic fmt` rewrite files, and
> `Cargo.toml` and `pyproject.toml` are checked against their schemas (M1-M5).
> The language server is the only milestone left.

## Why

- **Hand-written parser.** No TOML crate and no parser combinators: a lexer and
  a recursive descent parser built from the [TOML v1.0.0 spec](https://toml.io/en/v1.0.0).
- **Lossless tree.** Whitespace and comments are kept, so diagnostics point at
  exact positions and fixes edit the file without reformatting it.
- **Error recovery.** One typo does not hide every other error in the file.
- **Proven correct.** Validated against the official
  [`toml-test`](https://github.com/toml-lang/toml-test) suite and fuzzed.

## Usage

```sh
cargo install --path crates/kosmetic-cli

kosmetic check Cargo.toml pyproject.toml
kosmetic check --format json config.toml   # for CI and editors
kosmetic check --color never Cargo.toml    # auto (default), always, never
kosmetic check --fix Cargo.toml            # apply the automatic fixes
kosmetic fmt Cargo.toml                    # format in place
kosmetic fmt --check Cargo.toml            # CI: fail if a file would change
kosmetic fmt - < Cargo.toml                # stdin to stdout, for editors
kosmetic rules                             # list available rules
```

Exit codes: `0` no errors, `1` at least one error (`fmt --check`: a file
would change), `2` the run failed (unreadable file, bad arguments, invalid
configuration, a file `fmt` cannot format).

## Fixes and formatting

`check --fix` applies the fix of every enabled rule (see the table below)
and reports what is left. `fmt` only changes layout, and ignores the rule
configuration:

- `key = value`, `a.b.c`, `[table]`, `[[array.of.tables]]` with no stray spaces
- `[1, 2, 3]`, `{ a = 1, b = 2 }`, `[]` and `{}` for empty ones
- multi-line arrays: two spaces per level, a trailing comma, the closing
  bracket on its own line
- no indentation and no trailing whitespace anywhere else
- at most one blank line in a row, exactly one before every table header (or
  before the comments right above it), exactly one final newline
- the line ending of the first line (LF or CRLF) everywhere

Both commands are conservative. A file with errors is never touched, and after
every rewrite the file is parsed again: if its values (or its comments, for
`fmt`) would differ, nothing is written.

## Configuration

kosmetic reads the nearest `.kosmetic.toml` in the working directory or its
parents, or the file given with `--config`. Each rule can be set to `off`,
`warn` or `error`:

```toml
[rules]
"style/sorted-keys" = "warn"
"style/comment-space" = "off"
```

The file is parsed by kosmetic itself: a typo in a rule name is reported
with its position and stops the run.

## Rules

| Rule | Default | Fix |
| --- | --- | --- |
| `style/no-trailing-whitespace` | warn | yes |
| `style/final-newline` | warn | yes |
| `style/no-multiple-blank-lines` | warn | yes |
| `style/consistent-line-endings` | warn | yes |
| `style/spaces-around-equals` | warn | yes |
| `style/comment-space` | warn | yes |
| `style/prefer-bare-keys` | warn | yes |
| `style/sorted-keys` | off | no |
| `semantic/mixed-array-types` | warn | no |
| `schema/cargo-unknown-key` | warn | no |
| `schema/cargo-missing-field` | error | no |
| `schema/cargo-invalid-value` | error | no |
| `schema/cargo-wildcard-dependency` | warn | no |
| `schema/cargo-publish-metadata` | off | no |
| `schema/pyproject-unknown-key` | warn | no |
| `schema/pyproject-missing-field` | error | no |
| `schema/pyproject-invalid-value` | error | no |

Syntax errors (`syntax/...`) and semantic errors (`semantic/duplicate-key`,
`semantic/table-redefined`, `semantic/integer-overflow`,
`semantic/invalid-datetime`) are always errors and cannot be turned off.

## Schema rules

Files named `Cargo.toml` and `pyproject.toml` are also checked against what
Cargo and the Python packaging tools accept. The rules read the values of the
document, so they point at the exact key or value, and they come with a
"did you mean" hint for typos. Version syntax (semver, PEP 440, PEP 508) is
validated by hand-written parsers, no packaging crate is involved.

| Rule | What it checks |
| --- | --- |
| `schema/cargo-unknown-key` | keys Cargo does not know in the manifest, `[package]`, `[workspace]`, targets, profiles and dependencies |
| `schema/cargo-missing-field` | `package.name`, a `name` for every `[[bin]]`, `[[example]]`, `[[test]]` and `[[bench]]`, a source for every dependency table |
| `schema/cargo-invalid-value` | types, package name, semver version, `edition`, `rust-version`, `resolver`, version requirements, `git` + `path` conflicts, `branch` without `git`, `workspace = true` mixed with a version |
| `schema/cargo-wildcard-dependency` | `"*"` as a version requirement, which crates.io rejects |
| `schema/cargo-publish-metadata` | what crates.io needs to publish: `description`, `license`, at most 5 keywords and categories |
| `schema/pyproject-unknown-key` | unknown top-level tables (tools go under `[tool]`) and unknown keys in `[project]` and `[build-system]` |
| `schema/pyproject-missing-field` | `project.name`, `project.version` (unless dynamic), `build-system.requires` |
| `schema/pyproject-invalid-value` | types, project name, PEP 440 `version`, `requires-python`, PEP 508 requirements (dependencies, extras, build requirements, dependency groups), `dynamic` fields that are also set |

Fields inherited from the workspace (`version.workspace = true`) are accepted.
A field listed in `dynamic` and also set is reported, except with the Poetry
backend, which merges the two.

## Architecture

```text
source ─▶ lexer ─▶ tokens ─▶ parser ─▶ lossless tree ─▶ semantic model ─▶ rules ─▶ diagnostics
```

| Crate | Role |
| --- | --- |
| `kosmetic-diagnostics` | Spans, line index, diagnostics, human and JSON rendering |
| `kosmetic-syntax` | Lexer, tokens, lossless parser, value model and semantic checks |
| `kosmetic-rules` | `Rule` trait, rule engine, configuration, built-in rules (style, semantic and schema), `fix_source` |
| `kosmetic-format` | The formatter, built on the lossless tree |
| `kosmetic-cli` | The `kosmetic` binary |
| `kosmetic-toml-test` | Decoder adapter for the `toml-test` suite (not published) |

## Roadmap

- [x] **M1** Lexer and lossless parser passing the `toml-test` valid and invalid suites
- [x] **M2** Error recovery and rich diagnostics (multi-line spans, secondary labels, colors)
- [x] **M3** Semantic checks (duplicate keys, redefined tables), `.kosmetic.toml` config, 8-10 rules
- [x] **M4** `--fix` and a formatter built on the lossless tree
- [x] **M5** Schema rules for `Cargo.toml` and `pyproject.toml`
- [ ] **M6** Language server and VS Code extension (optional)

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
scripts/toml-test.sh   # official toml-test suite, needs Go
```

`toml-test` is a git submodule pinned to a release: clone with
`--recurse-submodules` or run `git submodule update --init`.

## License

[MIT](LICENSE)
