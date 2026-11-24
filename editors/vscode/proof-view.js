const REFRESH_DELAY_MS = 75;
const STATUS = {
  pending: "Verification pending",
  checked: "Checked",
  open: "Open obligation",
  blocked: "Verification blocked",
  "budget-exhausted": "Verification budget exhausted",
};

function proofRows(proof) {
  const rows = [{ label: proof.name, detail: STATUS[proof.status] }];
  if (proof.declaredGoal && proof.declaredGoal !== proof.goal) {
    rows.push({ label: "Declared goal", detail: proof.declaredGoal });
  }
  if (proof.goal) rows.push({ label: "Goal", detail: proof.goal });
  if (proof.class) rows.push({ label: "Class", detail: proof.class });
  if (proof.instance) rows.push({ label: "Instance", detail: proof.instance });
  if (proof.failureRange) {
    const { line, character } = proof.failureRange.start;
    rows.push({ label: "Failed step", detail: `Line ${line + 1}, column ${character + 1}` });
  }

  const sections = [
    ["Binders", proof.binders.map(({ name, type }) => ({ label: name, detail: type }))],
    ["Premises", proof.premises.map(({ name, statement }) => ({ label: name, detail: statement }))],
    ["Branch substitutions", proof.substitutions.map(({ name, value }) => ({ label: name, detail: value }))],
    ["Available lemmas", proof.lemmas.map((label) => ({ label }))],
    ["Law obligations", proof.obligations.map(({ path, law, status, statement }) => ({
      label: [...path, law].join("."),
      detail: [STATUS[status], statement].filter(Boolean).join(" · "),
    }))],
  ];
  for (const [label, children] of sections) {
    if (children.length) rows.push({ label, children });
  }
  return rows;
}

function flickFile(document) {
  return document?.languageId === "flick" && document.uri.scheme === "file";
}

function samePosition(a, b) {
  return a?.line === b.line && a?.character === b.character;
}

function createProofView(vscode, getClient, logError) {
  const changed = new vscode.EventEmitter();
  let rows = [];
  let timer;
  let pending;
  let minimumGeneration = 0;
  let disposed = false;

  const view = vscode.window.createTreeView("flick.proofGoals", {
    treeDataProvider: {
      onDidChangeTreeData: changed.event,
      getChildren: (row) => row ? row.children ?? [] : rows,
      getTreeItem(row) {
        const state = row.children ? vscode.TreeItemCollapsibleState.Expanded : vscode.TreeItemCollapsibleState.None;
        const item = new vscode.TreeItem(row.label, state);
        item.description = row.detail;
        item.tooltip = row.detail ?? row.label;
        return item;
      },
    },
  });

  function show(message, proof) {
    view.message = message;
    if (proof || rows.length) {
      rows = proof ? proofRows(proof) : [];
      changed.fire();
    }
  }

  function cancel() {
    clearTimeout(timer);
    pending?.cancel();
    pending = undefined;
  }

  async function query(client, editor) {
    const document = editor.document;
    const version = document.version;
    const { line, character } = editor.selection.active;
    const position = { line, character };
    const request = new vscode.CancellationTokenSource();
    pending = request;
    try {
      const textDocument = client.code2ProtocolConverter.asTextDocumentIdentifier(document);
      const result = await client.sendRequest("flick/proofAt", { textDocument, position }, request.token);
      if (pending !== request || !view.visible || getClient() !== client ||
          vscode.window.activeTextEditor !== editor || document.isClosed ||
          document.version !== version || !samePosition(editor.selection.active, position)) {
        return;
      }
      if (!result || !Number.isSafeInteger(result.generation) ||
          result.version !== version || !samePosition(result.position, position) ||
          result.generation < minimumGeneration) {
        return;
      }
      minimumGeneration = result.generation;
      if (result.pending) {
        show(STATUS.pending, result.proof);
      } else if (result.proof) {
        show(undefined, result.proof);
      } else {
        show("Move the cursor into a proof.");
      }
    } catch (error) {
      if (pending !== request) return;
      if (error?.code === -32601) {
        show("Proof Goals requires a newer Flick language server.");
      } else {
        show("Could not load the proof goal.");
        logError(`Failed to load proof goal: ${error instanceof Error ? error.message : String(error)}`);
      }
    } finally {
      if (pending === request) pending = undefined;
      request.dispose();
    }
  }

  function refresh() {
    cancel();
    if (disposed) return;
    if (!view.visible) {
      show(undefined);
      return;
    }
    const editor = vscode.window.activeTextEditor;
    if (!flickFile(editor?.document) || editor.document.isClosed) {
      show("Open a Flick file and move the cursor into a proof.");
      return;
    }
    const client = getClient();
    if (!client) {
      show("Waiting for the Flick language server.");
      return;
    }
    show("Loading proof goal…");
    timer = setTimeout(() => void query(client, editor), REFRESH_DELAY_MS);
  }

  const listeners = [
    view.onDidChangeVisibility(refresh),
    vscode.window.onDidChangeActiveTextEditor(refresh),
    vscode.window.onDidChangeTextEditorSelection(({ textEditor }) => {
      if (textEditor === vscode.window.activeTextEditor) refresh();
    }),
    vscode.workspace.onDidChangeTextDocument(({ document, contentChanges }) => {
      if (flickFile(document) && contentChanges.length) refresh();
    }),
    vscode.workspace.onDidCloseTextDocument((document) => {
      if (flickFile(document)) refresh();
    }),
  ];
  refresh();

  return {
    connectionChanged() {
      minimumGeneration = 0;
      refresh();
    },
    snapshotChanged(event) {
      const generation = event?.generation;
      if (Number.isSafeInteger(generation) && generation >= minimumGeneration) {
        minimumGeneration = generation;
        refresh();
      }
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      cancel();
      for (const listener of listeners) listener.dispose();
      view.dispose();
      changed.dispose();
    },
  };
}

module.exports = { createProofView };
