// A minimal client of `flick lsp --stdio`, for checking the server by hand.
// Built-in modules open as read-only `flick-builtin:` documents whose text
// the server gives.
const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

let client;

function activate(context) {
  const command = vscode.workspace.getConfiguration("flick").get("path", "flick");
  client = new LanguageClient(
    "flick",
    "Flick",
    { command, args: ["lsp", "--stdio"] },
    { documentSelector: [{ scheme: "file", language: "flick" }, { scheme: "flick-builtin" }] },
  );
  const builtins = {
    provideTextDocumentContent: async (uri) =>
      (await client.sendRequest("workspace/textDocumentContent", { uri: uri.toString() })).text,
  };
  context.subscriptions.push(vscode.workspace.registerTextDocumentContentProvider("flick-builtin", builtins));
  return client.start();
}

function deactivate() {
  return client?.stop();
}

module.exports = { activate, deactivate };
