# kosmetic for VS Code

Diagnostics, quick fixes and formatting for TOML files, from the
[kosmetic](https://github.com/GeneralKoski/kosmetic) language server.

- diagnostics while you type: syntax errors, semantic errors, style rules and
  the schema rules for `Cargo.toml` and `pyproject.toml`
- quick fixes for the problems the rules can fix, and a "fix all" action
- formatting of the whole document
- the nearest `.kosmetic.toml` configures the rules, and is checked itself

## Requirements

The `kosmetic` executable must be installed. The extension starts it as
`kosmetic lsp`:

```sh
cargo install --path crates/kosmetic-cli
```

If it is not on your `PATH`, set `kosmetic.path` in your user settings. The
setting is machine-scoped: workspace settings cannot change it, so an
untrusted repository cannot choose the program that runs.

## Settings

| Setting | Default | |
| --- | --- | --- |
| `kosmetic.path` | `kosmetic` | Path of the executable |
| `kosmetic.trace.server` | `off` | Traces the messages with the server |

The command **kosmetic: Restart Language Server** restarts it.

## Format and fix on save

```jsonc
{
  "[toml]": {
    "editor.defaultFormatter": "generalkoski.kosmetic",
    "editor.formatOnSave": true,
    "editor.codeActionsOnSave": { "source.fixAll": "explicit" }
  }
}
```

## Build

```sh
cd editors/vscode
npm install
npm run package   # produces kosmetic-0.1.0.vsix
```

The extension is plain JavaScript: there is no build step.
