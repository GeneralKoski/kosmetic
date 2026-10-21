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
> model pass the `toml-test` suite, diagnostics show labels and colors, and
> nine configurable rules run (M1-M3); `--fix` and the formatter come next.

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
kosmetic rules                             # list available rules
```

Exit codes: `0` no errors, `1` at least one error, `2` the run failed
(unreadable file, bad arguments, invalid configuration).

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

Syntax errors (`syntax/...`) and semantic errors (`semantic/duplicate-key`,
`semantic/table-redefined`, `semantic/integer-overflow`,
`semantic/invalid-datetime`) are always errors and cannot be turned off.

## Architecture

```text
source ─▶ lexer ─▶ tokens ─▶ parser ─▶ lossless tree ─▶ semantic model ─▶ rules ─▶ diagnostics
```

| Crate | Role |
| --- | --- |
| `kosmetic-diagnostics` | Spans, line index, diagnostics, human and JSON rendering |
| `kosmetic-syntax` | Lexer, tokens, lossless parser, value model and semantic checks |
| `kosmetic-rules` | `Rule` trait, rule engine, configuration, built-in rules |
| `kosmetic-cli` | The `kosmetic` binary |
| `kosmetic-toml-test` | Decoder adapter for the `toml-test` suite (not published) |

## Roadmap

- [x] **M1** Lexer and lossless parser passing the `toml-test` valid and invalid suites
- [x] **M2** Error recovery and rich diagnostics (multi-line spans, secondary labels, colors)
- [x] **M3** Semantic checks (duplicate keys, redefined tables), `.kosmetic.toml` config, 8-10 rules
- [ ] **M4** `--fix` and a formatter built on the lossless tree
- [ ] **M5** Schema rules for `Cargo.toml` and `pyproject.toml`
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
