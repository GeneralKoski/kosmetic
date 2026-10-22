# Changelog

## 0.1.0 - 2026-10-22

First release.

### Parser and diagnostics
- Hand-written lexer and lossless parser for TOML v1.0.0, passing the official
  `toml-test` suite (valid and invalid), with error recovery: one typo does not
  hide the other errors of a file.
- Semantic checks: duplicate keys, redefined tables, integers out of range and
  impossible dates.
- Diagnostics with labels, help texts, multi-line spans, colors, and a JSON output.

### Rules
- Nine style and semantic rules, configurable with `.kosmetic.toml`
  (`off`, `warn`, `error`).
- Schema rules for `Cargo.toml` and `pyproject.toml`: unknown keys with a
  "did you mean" hint, missing fields, invalid values (semver, PEP 440,
  PEP 508 and more), wildcard dependencies and the metadata crates.io needs.

### Command line
- `kosmetic check` with `--fix`, `--format json` and `--color`. A directory
  stands for the `.toml` files below it.
- `kosmetic fmt` with `--check`, `--diff` and `-` for standard input.
  The result is verified to hold the same values and comments as the input.
- `kosmetic lsp`: a language server with diagnostics, quick fixes, a "fix all"
  action and formatting.
- `kosmetic rules` lists the available rules.

### Editor
- VS Code extension in `editors/vscode`.
