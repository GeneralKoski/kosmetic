# kosmetic - note per Claude Code

Linter TOML in Rust, progetto di portfolio. Leggi il README per architettura e roadmap.

## Regole del progetto

- **Niente crate TOML o di parsing** (`toml`, `toml_edit`, `nom`, `winnow`, `logos`, ecc.)
  in `kosmetic-syntax`. Lexer e parser sono scritti a mano: è il senso del progetto.
  `toml` è ammesso solo come dev-dependency per confrontare i risultati nei test.
- **Offset in byte, `u32`.** Righe e colonne si calcolano solo in fase di rendering con `LineIndex`.
- **Lossless.** Gli span dei token, in ordine, devono coprire tutto il sorgente senza buchi.
- **Mai panic sull'input.** Qualsiasi input, anche binario, produce token `Error` e diagnostiche.
- **Codici diagnostica** nel formato `categoria/nome-in-kebab-case`
  (`syntax/`, `semantic/`, `style/`, `schema/`). Non si rinominano dopo il rilascio.
- Ogni nuova regola: un tipo che implementa `Rule`, registrato in `all_rules()`,
  con test che coprono caso positivo, negativo e `Level::Off`.
  `style::NoTrailingWhitespace` è l'esempio di riferimento.
- I `TODO(Mn)` indicano in quale milestone va fatto il lavoro.

## Prima di ogni commit

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

## Stile

- Codice, commenti, README e messaggi di diagnostica in inglese.
- Mai em dash né en dash, in nessun file: solo il trattino normale `-`.
