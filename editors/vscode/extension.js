'use strict';

const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

/** @type {LanguageClient | undefined} */
let client;

function createClient() {
  // Only the user-level value: a workspace must not choose what gets executed.
  const command = vscode.workspace.getConfiguration('kosmetic').inspect('path')?.globalValue || 'kosmetic';
  return new LanguageClient(
    'kosmetic',
    'kosmetic',
    { command, args: ['lsp'] },
    {
      documentSelector: [
        { scheme: 'file', language: 'toml' },
        { scheme: 'untitled', language: 'toml' },
      ],
      synchronize: {
        // The server reads the config again and republishes the diagnostics.
        fileEvents: vscode.workspace.createFileSystemWatcher('**/.kosmetic.toml'),
      },
    },
  );
}

async function start() {
  client = createClient();
  try {
    await client.start();
  } catch (error) {
    client = undefined;
    vscode.window.showErrorMessage(
      `kosmetic could not start the language server (${error.message}). ` +
        'Install it with `cargo install --path crates/kosmetic-cli` or set "kosmetic.path".',
    );
  }
}

async function stop() {
  const running = client;
  client = undefined;
  if (running) {
    await running.stop().catch(() => {});
  }
}

async function restart() {
  await stop();
  await start();
}

/** @param {vscode.ExtensionContext} context */
async function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand('kosmetic.restartServer', restart),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration('kosmetic.path')) {
        restart();
      }
    }),
  );
  await start();
}

function deactivate() {
  return stop();
}

module.exports = { activate, deactivate };
