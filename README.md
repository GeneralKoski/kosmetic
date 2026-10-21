# kosmetic

A fast TOML linter written in Rust, with a hand-written lexer and a lossless parser.

```text
$ kosmetic check Cargo.toml
Cargo.toml:2:18: warning[style/no-trailing-whitespace]: trailing whitespace
  2 | name = "kosmetic"
    |                  ^^^
  = help: remove the spaces at the end of the line
```

> Status: early development. The rule engine, diagnostics and CLI work; the
> lexer and parser are being written (milestone M1).

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
kosmetic rules                             # list available rules
```

Exit codes: `0` no errors, `1` at least one error, `2` the run failed
(unreadable file, bad arguments).

## Architecture

```text
source ─▶ lexer ─▶ tokens ─▶ parser ─▶ lossless tree ─▶ semantic model ─▶ rules ─▶ diagnostics
```

| Crate | Role |
| --- | --- |
| `kosmetic-diagnostics` | Spans, line index, diagnostics, human and JSON rendering |
| `kosmetic-syntax` | Lexer, tokens, lossless parser |
| `kosmetic-rules` | `Rule` trait, rule engine, configuration, built-in rules |
| `kosmetic-cli` | The `kosmetic` binary |

## Roadmap

- [ ] **M1** Lexer and lossless parser passing the `toml-test` valid and invalid suites
- [ ] **M2** Error recovery and rich diagnostics (multi-line spans, secondary labels, colors)
- [ ] **M3** Semantic checks (duplicate keys, redefined tables), `.kosmetic.toml` config, 8-10 rules
- [ ] **M4** `--fix` and a formatter built on the lossless tree
- [ ] **M5** Schema rules for `Cargo.toml` and `pyproject.toml`
- [ ] **M6** Language server and VS Code extension (optional)

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

## License

[MIT](LICENSE)
