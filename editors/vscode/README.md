# kosmetic for VS Code

Diagnostics, quick fixes and formatting for TOML files, from the
[kosmetic](https://github.com/GeneralKoski/kosmetic) language server.

- diagnostics while you type: syntax errors, semantic errors, style rules and
  the schema rules for `Cargo.toml` and `pyproject.toml`
- quick fixes for the problems the rules can fix, and a "fix all" action
- formatting of the whole document
- the nearest `.kosmetic.toml` configures the rules, and is checked itself

## Install the extension

The extension is published on [Open VSX](https://open-vsx.org/extension/generalkoski/kosmetic),
the open registry used by VSCodium, Cursor, Gitpod and other editors based on
VS Code. In those editors, search for **kosmetic** in the Extensions view and
install it.

It is **not** on the Visual Studio Marketplace, the registry of Microsoft's own
VS Code, which only accepts publishers that have an Azure subscription. In VS
Code, install it from the `.vsix` file attached to every
[release](https://github.com/GeneralKoski/kosmetic/releases):

```sh
code --install-extension kosmetic-0.1.0.vsix
```

or open the Extensions view, click `...` at the top, choose **Install from
VSIX...** and pick the file. Updates are not automatic this way: download the
new `.vsix` of each release and install it again.

## Requirements

The `kosmetic` executable must be installed. The extension starts it as
`kosmetic lsp`:

```sh
cargo install kosmetic-cli
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

The source is plain JavaScript in `src/extension.js`. `npm run build` bundles
it with its only dependency, `vscode-languageclient`, into `dist/extension.js`,
which is what the package contains (8 files, under 100 KB).
