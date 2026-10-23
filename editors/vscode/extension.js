// A minimal client of `flick lsp --stdio`, for checking the server by hand.
const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

let client;

function activate() {
  const command = vscode.workspace.getConfiguration("flick").get("path", "flick");
  client = new LanguageClient(
    "flick",
    "Flick",
    { command, args: ["lsp", "--stdio"] },
    { documentSelector: [{ scheme: "file", language: "flick" }] },
  );
  return client.start();
}

function deactivate() {
  return client?.stop();
}

module.exports = { activate, deactivate };
