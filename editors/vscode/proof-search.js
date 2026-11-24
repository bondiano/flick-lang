// The command previews checked source and validates the same revision before inserting it.

function flickFile(document) {
  return document?.languageId === "flick" && document.uri.scheme === "file";
}

function samePosition(a, b) {
  return a?.line === b.line && a?.character === b.character;
}

function validPosition(at) {
  return Number.isSafeInteger(at?.line) && at.line >= 0 &&
    Number.isSafeInteger(at.character) && at.character >= 0;
}

function createProofSearch(vscode, getClient, logError) {
  let active;
  let minimumGeneration = 0;
  let disposed = false;

  function cancel() {
    const previous = active;
    active = undefined;
    previous?.source.cancel();
  }

  function current(search) {
    const { client, editor, document, version, position, source } = search;
    return active === search && !source.token.isCancellationRequested &&
      getClient() === client && vscode.window.activeTextEditor === editor && !document.isClosed &&
      document.version === version && samePosition(editor.selection.active, position);
  }

  async function run() {
    if (disposed) return;
    cancel();
    const editor = vscode.window.activeTextEditor;
    const document = editor?.document;
    if (!flickFile(document) || document.isClosed) {
      void vscode.window.showInformationMessage("Open a Flick file and place the cursor in a proof.");
      return;
    }
    const client = getClient();
    if (!client) {
      void vscode.window.showInformationMessage("Waiting for the Flick language server.");
      return;
    }
    const { line, character } = editor.selection.active;
    const position = { line, character };
    const version = document.version;
    const source = new vscode.CancellationTokenSource();
    const search = { client, editor, document, version, position, source };
    active = search;
    try {
      const textDocument = client.code2ProtocolConverter.asTextDocumentIdentifier(document);
      const result = await vscode.window.withProgress({
        location: vscode.ProgressLocation.Notification,
        title: "Searching for a Flick proof",
        cancellable: true,
      }, async (_progress, cancellation) => {
        const listener = cancellation.onCancellationRequested(() => source.cancel());
        try {
          return await client.sendRequest("flick/searchProof", { textDocument, position }, source.token);
        } finally {
          listener.dispose();
        }
      });
      if (!current(search) || !result || result.version !== version || !samePosition(result.position, position) ||
          !Number.isSafeInteger(result.generation) || result.generation < minimumGeneration) return;
      search.generation = result.generation;
      minimumGeneration = result.generation;
      if (result.status === "cancelled") return;
      if (result.status === "budget-exhausted") {
        void vscode.window.showInformationMessage("Proof search reached its limit.");
        return;
      }
      if (result.status !== "found" || !Array.isArray(result.suggestions)) {
        void vscode.window.showInformationMessage("No checked proof term was found.");
        return;
      }
      const items = result.suggestions.filter(({ text, range }) =>
        typeof text === "string" && text.length > 0 && validPosition(range?.start) && validPosition(range?.end),
      ).map(({ text, range }) => ({ label: text, range }));
      if (!items.length) return;
      const selected = await vscode.window.showQuickPick(items, {
        placeHolder: "Select a checked proof term to insert",
      }, source.token);
      if (!current(search) || !items.includes(selected)) return;
      const fresh = await client.sendRequest("flick/validateProofSearch", {
        textDocument, version, generation: result.generation,
      }, source.token);
      if (!current(search)) return;
      if (fresh !== true) {
        void vscode.window.showInformationMessage("The proof context changed. Run Find Proof again.");
        return;
      }
      const { start, end } = selected.range;
      const range = new vscode.Range(start.line, start.character, end.line, end.character);
      if (!samePosition(range.start, start) || !samePosition(range.end, end) ||
          !document.validateRange(range).isEqual(range)) return;
      const applied = await editor.edit((edit) => edit.replace(range, selected.label));
      if (!applied && current(search)) {
        void vscode.window.showInformationMessage("The proof changed before the edit could be applied.");
      }
    } catch (error) {
      if (!current(search) || error?.code === -32800) return;
      if (error?.code === -32601) {
        void vscode.window.showInformationMessage("Find Proof requires a newer Flick language server.");
      } else {
        void vscode.window.showErrorMessage("Could not search for a proof.");
        logError(`Failed to search for a proof: ${error instanceof Error ? error.message : String(error)}`);
      }
    } finally {
      if (active === search) active = undefined;
      source.dispose();
    }
  }

  const listeners = [
    vscode.window.onDidChangeActiveTextEditor(cancel),
    vscode.window.onDidChangeTextEditorSelection(({ textEditor }) => {
      if (active?.editor === textEditor && !samePosition(textEditor.selection.active, active.position)) cancel();
    }),
    vscode.workspace.onDidChangeTextDocument(({ document, contentChanges }) => {
      if (flickFile(document) && contentChanges.length) cancel();
    }),
    vscode.workspace.onDidCloseTextDocument((document) => { if (flickFile(document)) cancel(); }),
  ];
  return {
    run,
    connectionChanged() {
      minimumGeneration = 0;
      cancel();
    },
    snapshotChanged(event) {
      if (!Number.isSafeInteger(event?.generation) || event.generation <= minimumGeneration) return;
      minimumGeneration = event.generation;
      if (active?.generation !== undefined && active.generation < minimumGeneration) cancel();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      cancel();
      for (const listener of listeners) listener.dispose();
    },
  };
}

module.exports = { createProofSearch };
