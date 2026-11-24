// Verifies VS Code extension manifest contributions and extension client
// lifecycle ergonomics (`flick.restartServer`, configuration change listener,
// status bar states, file system watchers, output channel, and cleanup).
//
//   node --test editors/vscode/scripts/extension.test.mjs

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import Module, { createRequire } from "node:module";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
const extensionPath = path.join(root, "extension.js");

function createHarness({
  initialConfig = {},
  failStartOn = () => false,
  hangStartOn = () => false,
  stopDelayMs = 0,
} = {}) {
  const State = { Stopped: 1, Running: 2, Starting: 3, StartFailed: 4 };
  const CloseAction = { DoNotRestart: 1, Restart: 2 };
  const ErrorAction = { Continue: 1, Shutdown: 2 };
  const configValues = {
    path: "flick",
    "trace.server": "off",
    ...initialConfig,
  };

  const registeredCommands = new Map();
  const configListeners = [];
  const createdWatchers = [];
  const createdClients = [];
  const createdStatusBars = [];
  const createdChannels = [];
  const createdViews = [];
  const searches = [];
  const previews = [];
  const information = [];

  class FakeEventEmitter {
    listeners = new Set();
    event = (listener) => {
      this.listeners.add(listener);
      return { dispose: () => this.listeners.delete(listener) };
    };
    fire(value) {
      for (const listener of [...this.listeners]) listener(value);
    }
    dispose() {
      this.listeners.clear();
    }
  }

  const activeEditorChanged = new FakeEventEmitter();
  const selectionChanged = new FakeEventEmitter();
  const documentChanged = new FakeEventEmitter();
  const documentClosed = new FakeEventEmitter();

  class FakeThemeColor {
    constructor(id) {
      this.id = id;
    }
  }

  class FakeOutputChannel {
    constructor(name, options) {
      this.name = name;
      this.options = options;
      this.logs = [];
      this.shown = false;
      this.disposed = false;
    }
    info(msg) {
      this.logs.push(["info", msg]);
    }
    error(msg) {
      this.logs.push(["error", msg]);
    }
    appendLine(msg) {
      this.logs.push(["line", msg]);
    }
    show(preserveFocus) {
      this.shown = preserveFocus ?? true;
    }
    dispose() {
      this.disposed = true;
    }
  }

  class FakeStatusBarItem {
    constructor(id, alignment, priority) {
      this.id = id;
      this.alignment = alignment;
      this.priority = priority;
      this.text = "";
      this.tooltip = undefined;
      this.command = undefined;
      this.backgroundColor = undefined;
      this.visible = false;
      this.disposed = false;
    }
    show() {
      this.visible = true;
    }
    dispose() {
      this.disposed = true;
    }
  }

  class FakeLanguageClient {
    constructor(id, name, serverOptions, clientOptions) {
      this.id = id;
      this.name = name;
      this.serverOptions = serverOptions;
      this.clientOptions = clientOptions;
      this.state = State.Stopped;
      this.startCalls = 0;
      this.stopCalls = 0;
      this.disposeCalls = 0;
      this.stateListeners = [];
      this.serverProcess = undefined;
      this.killedSignals = [];
      this.requests = [];
      this.notifications = new Map();
      this.code2ProtocolConverter = {
        asTextDocumentIdentifier: (document) => ({ uri: document.uri.toString() }),
      };
      createdClients.push(this);
    }
    onNotification(method, handler) {
      this.notifications.set(method, handler);
      return { dispose: () => this.notifications.delete(method) };
    }
    sendRequest(method, params, token) {
      return new Promise((resolve, reject) => this.requests.push({ method, params, token, resolve, reject }));
    }
    onDidChangeState(listener) {
      this.stateListeners.push(listener);
      return {
        dispose: () => {
          const idx = this.stateListeners.indexOf(listener);
          if (idx >= 0) this.stateListeners.splice(idx, 1);
        },
      };
    }
    emitState(newState) {
      const oldState = this.state;
      this.state = newState;
      for (const listener of [...this.stateListeners]) {
        listener({ oldState, newState });
      }
    }
    async start() {
      this.startCalls += 1;
      this.emitState(State.Starting);
      const proc = {
        pid: 10000 + createdClients.length,
        killed: false,
        kill: (signal = "SIGTERM") => {
          proc.killed = true;
          this.killedSignals.push(signal);
        },
      };
      this.serverProcess = proc;
      if (hangStartOn(this.serverOptions.command, this)) {
        return new Promise(() => {});
      }
      if (failStartOn(this.serverOptions.command, this)) {
        this.emitState(State.StartFailed);
        throw new Error(`spawn ${this.serverOptions.command} ENOENT`);
      }
      this.emitState(State.Running);
    }
    async stop() {
      if (this.state === State.StartFailed || this.state === State.Starting) {
        const label = this.state === State.StartFailed ? "startFailed" : "starting";
        throw new Error(`Client is not running and can't be stopped. It's current state is: ${label}`);
      }
      if (this.state === State.Stopped) {
        return;
      }
      this.stopCalls += 1;
      if (stopDelayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, stopDelayMs));
      }
      this.emitState(State.Stopped);
    }
    async dispose() {
      this.disposeCalls += 1;
      await this.stop();
    }
  }

  const fakeVscode = {
    StatusBarAlignment: { Left: 1, Right: 2 },
    ThemeColor: FakeThemeColor,
    EventEmitter: FakeEventEmitter,
    CancellationTokenSource: class {
      event = new FakeEventEmitter();
      token = { isCancellationRequested: false, onCancellationRequested: this.event.event };
      cancel() {
        if (this.token.isCancellationRequested) return;
        this.token.isCancellationRequested = true;
        this.event.fire();
      }
      dispose() { this.event.dispose(); }
    },
    ProgressLocation: { Notification: 15 },
    Range: class {
      constructor(startLine, startCharacter, endLine, endCharacter) {
        this.start = { line: startLine, character: startCharacter };
        this.end = { line: endLine, character: endCharacter };
      }
      isEqual(other) { return JSON.stringify(this) === JSON.stringify(other); }
    },
    TreeItemCollapsibleState: { None: 0, Expanded: 2 },
    TreeItem: class {
      constructor(label, collapsibleState) {
        this.label = label;
        this.collapsibleState = collapsibleState;
      }
    },
    window: {
      activeTextEditor: undefined,
      onDidChangeActiveTextEditor: activeEditorChanged.event,
      onDidChangeTextEditorSelection: selectionChanged.event,
      withProgress(options, task) {
        const source = new fakeVscode.CancellationTokenSource();
        searches.push({ options, source });
        return task({}, source.token).finally(() => source.dispose());
      },
      showQuickPick(items, options, token) {
        return new Promise((resolve) => {
          const listener = token.onCancellationRequested(() => resolve(undefined));
          previews.push({
            items, options, token,
            select(index) { listener.dispose(); resolve(items[index]); },
          });
        });
      },
      showInformationMessage(message) { information.push(message); },
      showErrorMessage(message) { information.push(message); },
      createTreeView(id, { treeDataProvider }) {
        const visibilityChanged = new FakeEventEmitter();
        const view = {
          id,
          visible: false,
          onDidChangeVisibility: visibilityChanged.event,
          setVisible(visible) {
            this.visible = visible;
            visibilityChanged.fire({ visible });
          },
          items(parent) {
            return treeDataProvider.getChildren(parent).map((row) => treeDataProvider.getTreeItem(row));
          },
          children(label) {
            return this.items(treeDataProvider.getChildren().find((row) => row.label === label));
          },
          dispose() {
            this.disposed = true;
            visibilityChanged.dispose();
          },
        };
        createdViews.push(view);
        return view;
      },
      createOutputChannel(name, options) {
        const channel = new FakeOutputChannel(name, options);
        createdChannels.push(channel);
        return channel;
      },
      createStatusBarItem(id, alignment, priority) {
        const item = new FakeStatusBarItem(id, alignment, priority);
        createdStatusBars.push(item);
        return item;
      },
    },
    commands: {
      registerCommand(command, handler) {
        registeredCommands.set(command, handler);
        return {
          dispose() {
            registeredCommands.delete(command);
          },
        };
      },
    },
    workspace: {
      onDidChangeTextDocument: documentChanged.event,
      onDidCloseTextDocument: documentClosed.event,
      getConfiguration(section) {
        assert.equal(section, "flick");
        return {
          get(key, fallback) {
            return key in configValues ? configValues[key] : fallback;
          },
        };
      },
      onDidChangeConfiguration(listener) {
        configListeners.push(listener);
        return {
          dispose() {
            const idx = configListeners.indexOf(listener);
            if (idx >= 0) configListeners.splice(idx, 1);
          },
        };
      },
      createFileSystemWatcher(globPattern) {
        const watcher = { globPattern, disposed: false, dispose() { this.disposed = true; } };
        createdWatchers.push(watcher);
        return watcher;
      },
    },
  };

  // Load a fresh instance of extension.js with mocked `vscode` and `vscode-languageclient/node`.
  delete require.cache[extensionPath];
  const originalLoad = Module._load;
  Module._load = function patchedLoad(request, parent, isMain) {
    if (request === "vscode") return fakeVscode;
    if (request === "vscode-languageclient/node") {
      return { LanguageClient: FakeLanguageClient, State, CloseAction, ErrorAction };
    }
    return originalLoad.call(this, request, parent, isMain);
  };

  let extension;
  try {
    extension = require(extensionPath);
  } finally {
    Module._load = originalLoad;
  }

  return {
    extension,
    configValues,
    registeredCommands,
    createdClients,
    createdStatusBars,
    createdChannels,
    createdWatchers,
    createdViews,
    searches,
    previews,
    information,
    setActiveEditor(editor) {
      fakeVscode.window.activeTextEditor = editor;
      activeEditorChanged.fire(editor);
    },
    setCursor(editor, line, character) {
      editor.selection = { active: { line, character } };
      selectionChanged.fire({ textEditor: editor });
    },
    changeDocument(document) {
      document.version += 1;
      documentChanged.fire({ document, contentChanges: [{}] });
    },
    closeDocument(document) {
      document.isClosed = true;
      documentClosed.fire(document);
    },
    fireConfigChange(affectedSection) {
      const event = {
        affectsConfiguration(section) {
          return affectedSection === section || affectedSection.startsWith(`${section}.`);
        },
      };
      for (const listener of [...configListeners]) {
        listener(event);
      }
    },
  };
}

test("package.json contributes commands, configuration, and grammar", () => {
  const commands = manifest.contributes?.commands ?? [];
  const restart = commands.find((c) => c.command === "flick.restartServer");
  assert.ok(restart, "flick.restartServer command must be contributed");
  assert.equal(restart.title, "Restart Language Server");
  assert.equal(restart.category, "Flick");

  const showOutput = commands.find((c) => c.command === "flick.showOutput");
  assert.ok(showOutput, "flick.showOutput command must be contributed");
  assert.equal(commands.find((c) => c.command === "flick.searchProof")?.title, "Find Proof");
  assert.ok(manifest.contributes?.views?.explorer?.some(({ id }) => id === "flick.proofGoals"));

  const props = manifest.contributes?.configuration?.properties ?? {};
  assert.ok(props["flick.path"], "flick.path setting must exist");
  assert.ok(props["flick.trace.server"], "flick.trace.server setting must exist");
  assert.deepEqual(props["flick.trace.server"].enum, ["off", "messages", "verbose"]);
  assert.equal(manifest.main, "./dist/extension.js");
});

test("activate registers commands, watchers, output channel, and status bar ready state", async () => {
  const h = createHarness();
  const context = { subscriptions: [] };

  await h.extension.activate(context);

  assert.equal(h.createdChannels.length, 1);
  assert.equal(h.createdChannels[0].name, "Flick");
  assert.deepEqual(h.createdChannels[0].options, { log: true });

  assert.equal(h.createdStatusBars.length, 1);
  const bar = h.createdStatusBars[0];
  assert.equal(bar.id, "flick.statusBar");
  assert.equal(bar.visible, true);
  assert.match(bar.text, /Ready/);
  assert.equal(bar.command, "flick.showOutput");
  assert.ok(bar.tooltip);

  assert.ok(h.registeredCommands.has("flick.restartServer"));
  assert.ok(h.registeredCommands.has("flick.showOutput"));

  assert.equal(h.createdClients.length, 1);
  const client = h.createdClients[0];
  assert.deepEqual(client.serverOptions, { command: "flick", args: ["lsp", "--stdio"] });
  assert.deepEqual(client.clientOptions.documentSelector, [
    { scheme: "file", language: "flick" },
    { scheme: "untitled", language: "flick" },
  ]);
  assert.equal(client.clientOptions.outputChannel, h.createdChannels[0]);
  assert.equal(client.clientOptions.traceOutputChannel, h.createdChannels[0]);
  assert.deepEqual(
    h.createdWatchers.map((w) => w.globPattern),
    ["**/*.fl", "**/flick.toml", "**/flick.lock"],
  );

  // `flick.showOutput` opens the output channel.
  h.registeredCommands.get("flick.showOutput")();
  assert.equal(h.createdChannels[0].shown, true);

  await h.extension.deactivate();
  assert.equal(client.disposeCalls, 1);
  assert.equal(bar.disposed, true);
  assert.equal(h.createdChannels[0].disposed, true);
  assert.ok(h.createdWatchers.every((w) => w.disposed));
});

test("status bar reflects Busy during in-flight requests and Error on startup failure", async () => {
  const h = createHarness({
    initialConfig: { path: "/missing/flick" },
    failStartOn: (cmd) => cmd === "/missing/flick",
  });
  const context = { subscriptions: [] };

  await h.extension.activate(context);
  const bar = h.createdStatusBars[0];
  assert.match(bar.text, /Error/);
  assert.equal(bar.command, "flick.restartServer");
  assert.equal(bar.backgroundColor?.id, "statusBarItem.errorBackground");

  // Fixing flick.path and triggering flick.restartServer recovers cleanly to Ready
  // without calling dispose() on the StartFailed client or logging a stop error.
  h.configValues.path = "/usr/local/bin/flick";
  await h.registeredCommands.get("flick.restartServer")();
  assert.equal(h.createdClients.length, 2);
  assert.equal(h.createdClients[0].disposeCalls, 0);
  assert.equal(
    h.createdChannels[0].logs.some(([level, msg]) => level === "error" && msg.includes("Failed to stop")),
    false,
    "recovering from StartFailed must not log a spurious stop error",
  );
  assert.deepEqual(h.createdClients[1].serverOptions, {
    command: "/usr/local/bin/flick",
    args: ["lsp", "--stdio"],
  });
  assert.match(bar.text, /Ready/);
  assert.equal(bar.command, "flick.showOutput");
  assert.equal(bar.backgroundColor, undefined);

  // Middleware marks status bar Busy while an LSP request is in flight.
  const sendRequest = h.createdClients[1].clientOptions.middleware.sendRequest;
  let resolveReq;
  const pending = sendRequest("textDocument/hover", {}, undefined, () => {
    return new Promise((resolve) => {
      resolveReq = resolve;
    });
  });
  assert.match(bar.text, /Busy/);
  resolveReq({ contents: "Int" });
  await pending;
  assert.match(bar.text, /Ready/);

  await h.extension.deactivate();
});

test("configuration listener restarts only when relevant Flick settings change", async () => {
  const h = createHarness();
  const context = { subscriptions: [] };

  await h.extension.activate(context);
  assert.equal(h.createdClients.length, 1);

  // Unrelated configuration change does not restart the server.
  h.fireConfigChange("editor.tabSize");
  await new Promise((r) => setTimeout(r, 10));
  assert.equal(h.createdClients.length, 1);

  // Flick event without actual setting change does not restart the server.
  h.fireConfigChange("flick");
  await new Promise((r) => setTimeout(r, 10));
  assert.equal(h.createdClients.length, 1);

  // Changing `flick.path` triggers a clean restart with the new binary.
  h.configValues.path = "/opt/flick/bin/flick";
  h.fireConfigChange("flick.path");
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(h.createdClients.length, 2);
  assert.equal(h.createdClients[0].disposeCalls, 1);
  assert.equal(h.createdClients[1].serverOptions.command, "/opt/flick/bin/flick");

  // Changing `flick.trace.server` triggers a clean restart.
  h.configValues["trace.server"] = "verbose";
  h.fireConfigChange("flick.trace.server");
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(h.createdClients.length, 3);
  assert.equal(h.createdClients[1].disposeCalls, 1);

  await h.extension.deactivate();
});

test("deactivate race ignores restartServer and configuration changes without leaking clients or watchers", async () => {
  const h = createHarness({ stopDelayMs: 30 });
  const context = { subscriptions: [] };

  await h.extension.activate(context);
  assert.equal(h.createdClients.length, 1);

  // Trigger deactivate while stopCurrentClient is awaiting an async stop.
  const deactivatePromise = h.extension.deactivate();
  h.configValues.path = "/opt/flick/bin/flick-after-deactivate";
  h.fireConfigChange("flick.path");
  await h.extension.restartServer();
  await deactivatePromise;

  // Give any erroneously queued tasks a chance to run.
  await new Promise((resolve) => setTimeout(resolve, 40));

  assert.equal(h.createdClients.length, 1, "no new client may be spawned after deactivate() starts");
  assert.ok(h.createdWatchers.every((w) => w.disposed), "all watchers must remain disposed after deactivate()");
  assert.equal(h.createdWatchers.length, 3);
  assert.equal(h.registeredCommands.size, 0, "commands should be disposed on deactivate()");
});

test("StartFailed recovery and deactivation skip client stop/dispose without spurious error logs", async () => {
  const h = createHarness({
    initialConfig: { path: "/bad/flick" },
    failStartOn: (cmd) => cmd === "/bad/flick",
  });
  await h.extension.activate({ subscriptions: [] });
  assert.match(h.createdStatusBars[0].text, /Error/);

  // Deactivating directly from StartFailed must not throw or log a stop error.
  await h.extension.deactivate();
  assert.equal(h.createdClients[0].disposeCalls, 0);
  assert.equal(
    h.createdChannels[0].logs.some(([level, msg]) => level === "error" && msg.includes("Failed to stop")),
    false,
  );
  assert.ok(h.createdWatchers.every((w) => w.disposed));
});

test("hung startup is preempted by restartServer and deactivate without deadlocking", async () => {
  const h = createHarness({
    initialConfig: { path: "/hung/cat" },
    hangStartOn: (cmd) => cmd === "/hung/cat",
  });

  const activatePromise = h.extension.activate({ subscriptions: [] });
  // Yield so startNewClient begins awaiting the hung start().
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(h.createdClients.length, 1);
  assert.match(h.createdStatusBars[0].text, /Starting/);

  // Updating flick.path and triggering restartServer must preempt the hung start immediately.
  h.configValues.path = "/good/flick";
  const restartPromise = h.registeredCommands.get("flick.restartServer")();

  await Promise.race([
    Promise.all([activatePromise, restartPromise]),
    new Promise((_, reject) => setTimeout(() => reject(new Error("restartServer deadlocked behind hung startup")), 500)),
  ]);

  assert.equal(h.createdClients.length, 2);
  assert.equal(h.createdClients[0].serverProcess.killed, true, "hung server process must be terminated on preemption");
  assert.match(h.createdStatusBars[0].text, /Ready/);

  // Now test deactivate() preempting another hung startup.
  h.configValues.path = "/hung/cat";
  const hungRestart = h.registeredCommands.get("flick.restartServer")();
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(h.createdClients.length, 3);

  await Promise.race([
    Promise.all([hungRestart, h.extension.deactivate()]),
    new Promise((_, reject) => setTimeout(() => reject(new Error("deactivate deadlocked behind hung startup")), 500)),
  ]);

  assert.equal(h.createdClients[2].serverProcess.killed, true);
  assert.ok(h.createdWatchers.every((w) => w.disposed));
});

test("hung startup times out cleanly, kills server process, sets Error status, and allows recovery", async (t) => {
  const prevTimeout = process.env.FLICK_LSP_START_TIMEOUT_MS;
  process.env.FLICK_LSP_START_TIMEOUT_MS = "40";
  t.after(() => {
    if (prevTimeout === undefined) {
      delete process.env.FLICK_LSP_START_TIMEOUT_MS;
    } else {
      process.env.FLICK_LSP_START_TIMEOUT_MS = prevTimeout;
    }
  });

  const h = createHarness({
    initialConfig: { path: "/hung/sleep" },
    hangStartOn: (cmd) => cmd === "/hung/sleep",
  });

  await Promise.race([
    h.extension.activate({ subscriptions: [] }),
    new Promise((_, reject) => setTimeout(() => reject(new Error("activate hung past startup timeout")), 500)),
  ]);

  assert.equal(h.createdClients.length, 1);
  assert.equal(h.createdClients[0].serverProcess.killed, true);
  assert.match(h.createdStatusBars[0].text, /Error/);
  assert.equal(h.createdStatusBars[0].command, "flick.restartServer");
  assert.ok(
    h.createdChannels[0].logs.some(([level, msg]) => level === "error" && /timed out/i.test(msg)),
    "timeout must be logged to output channel",
  );

  // Recovery after timeout must succeed without logging a spurious stop error.
  h.configValues.path = "/good/flick";
  await h.registeredCommands.get("flick.restartServer")();
  assert.equal(h.createdClients.length, 2);
  assert.match(h.createdStatusBars[0].text, /Ready/);
  assert.equal(
    h.createdChannels[0].logs.some(([level, msg]) => level === "error" && msg.includes("Failed to stop")),
    false,
  );

  await h.extension.deactivate();
});

test("synchronous throw in middleware sendRequest next() resets status bar from Busy to Ready", async () => {
  const h = createHarness();
  await h.extension.activate({ subscriptions: [] });
  const bar = h.createdStatusBars[0];
  const sendRequest = h.createdClients[0].clientOptions.middleware.sendRequest;

  await assert.rejects(
    () => Promise.resolve(sendRequest("textDocument/hover", {}, undefined, () => {
      throw new Error("sync boom");
    })),
    /sync boom/,
  );
  assert.match(bar.text, /Ready/);

  await h.extension.deactivate();
});

function flickDocument(file) {
  return { languageId: "flick", version: 1, uri: { scheme: "file", toString: () => `file:///${file}` } };
}

function proofReply(request, generation, version, overrides = {}) {
  return {
    generation,
    version,
    position: request.params.position,
    pending: false,
    proof: {
      name: "back",
      status: "checked",
      declaredGoal: "y = x",
      goal: "x = y",
      binders: [{ name: "x", type: "Bool" }],
      premises: [{ name: "h", statement: "x = y" }],
      substitutions: [{ name: "v", value: "Some x" }],
      lemmas: ["Laws.identity"],
      instance: null,
      obligations: [],
      ...overrides,
    },
  };
}

test("Proof Goals follows the active cursor and distinguishes pending verification from an open goal", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const h = createHarness();
  await h.extension.activate({ subscriptions: [] });
  t.after(() => h.extension.deactivate());
  const [client] = h.createdClients;
  const [view] = h.createdViews;
  const document = flickDocument("Main.fl");
  const editor = { document, selection: { active: { line: 2, character: 12 } } };
  h.setActiveEditor(editor);
  t.mock.timers.tick(100);
  assert.equal(client.requests.length, 0, "the collapsed view does no work");

  view.setVisible(true);
  t.mock.timers.tick(100);
  const pending = client.requests.at(-1);
  assert.equal(pending.method, "flick/proofAt");
  assert.deepEqual(pending.params, {
    textDocument: { uri: "file:///Main.fl" }, position: { line: 2, character: 12 },
  });
  pending.resolve({ ...proofReply(pending, 2, 1), proof: null, pending: true });
  await Promise.resolve();
  assert.equal(view.message, "Verification pending");
  assert.deepEqual(view.items(), []);

  client.notifications.get("flick/proofStateChanged")({ generation: 2 });
  t.mock.timers.tick(100);
  const open = client.requests.at(-1);
  assert.notEqual(open, pending);
  open.resolve(proofReply(open, 2, 1, { status: "open", goal: "y = x" }));
  await Promise.resolve();
  assert.equal(view.message, undefined);
  assert.deepEqual(view.items().slice(0, 2).map(({ label, description }) => [label, description]), [
    ["back", "Open obligation"], ["Goal", "y = x"],
  ]);

  h.setCursor(editor, 3, 9);
  t.mock.timers.tick(100);
  const inside = client.requests.at(-1);
  assert.deepEqual(inside.params.position, { line: 3, character: 9 });
  inside.resolve(proofReply(inside, 2, 1));
  await Promise.resolve();
  assert.deepEqual(view.items().slice(0, 3).map(({ label, description }) => [label, description]), [
    ["back", "Checked"], ["Declared goal", "y = x"], ["Goal", "x = y"],
  ]);
  assert.equal(view.children("Binders")[0].description, "Bool");
  assert.equal(view.children("Premises")[0].description, "x = y");
  assert.equal(view.children("Branch substitutions")[0].description, "Some x");
  assert.equal(view.children("Available lemmas")[0].label, "Laws.identity");

  view.setVisible(false);
  assert.deepEqual(view.items(), [], "reopening the view must start without the previous goal");
});

test("Proof Goals discards results superseded by dependency changes, edits and close", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const h = createHarness();
  await h.extension.activate({ subscriptions: [] });
  t.after(() => h.extension.deactivate());
  const [client] = h.createdClients;
  const [view] = h.createdViews;
  const document = flickDocument("Main.fl");
  const editor = { document, selection: { active: { line: 2, character: 12 } } };
  h.setActiveEditor(editor);
  view.setVisible(true);
  t.mock.timers.tick(100);

  const superseded = client.requests.at(-1);
  const snapshotChanged = client.notifications.get("flick/proofStateChanged");
  snapshotChanged({ generation: 3 });
  assert.equal(superseded.token.isCancellationRequested, true);
  superseded.resolve(proofReply(superseded, 2, 1));
  await Promise.resolve();
  assert.deepEqual(view.items(), []);

  t.mock.timers.tick(100);
  const oldGeneration = client.requests.at(-1);
  oldGeneration.resolve(proofReply(oldGeneration, 2, 1));
  await Promise.resolve();
  assert.deepEqual(view.items(), [], "a delayed snapshot cannot restore checked status");

  h.changeDocument(document);
  t.mock.timers.tick(100);
  const oldVersion = client.requests.at(-1);
  oldVersion.resolve(proofReply(oldVersion, 3, 1));
  await Promise.resolve();
  assert.deepEqual(view.items(), [], "the reply must match the editor document version");

  snapshotChanged({ generation: 4 });
  t.mock.timers.tick(100);
  const bundle = client.requests.at(-1);
  bundle.resolve(proofReply(bundle, 4, 2, {
    name: "monad",
    status: "blocked",
    instance: "Monad Option",
    failureRange: { start: { line: 4, character: 9 }, end: { line: 4, character: 10 } },
    obligations: [{ path: ["Applicative", "Functor"], law: "mapIdentity", status: "open", statement: null }],
  }));
  await Promise.resolve();
  assert.equal(view.items()[0].description, "Verification blocked");
  assert.equal(view.items().find(({ label }) => label === "Instance").description, "Monad Option");
  assert.equal(view.items().find(({ label }) => label === "Failed step").description, "Line 5, column 10");
  assert.equal(view.children("Law obligations")[0].label, "Applicative.Functor.mapIdentity");

  h.setCursor(editor, 4, 14);
  t.mock.timers.tick(100);
  const afterClose = client.requests.at(-1);
  h.closeDocument(document);
  afterClose.resolve(proofReply(afterClose, 4, 2));
  await Promise.resolve();
  assert.equal(afterClose.token.isCancellationRequested, true);
  assert.equal(view.message, "Open a Flick file and move the cursor into a proof.");
  assert.deepEqual(view.items(), []);
});

const OPEN_PROOF = "proof back (h : equal true true) where equal true true = todo";
const PROOF_BODY = OPEN_PROOF.indexOf("todo");
const turn = () => new Promise((resolve) => setImmediate(resolve));

function searchEditor() {
  const document = {
    ...flickDocument("Proofs.fl"),
    text: OPEN_PROOF,
    validateRange: (range) => range,
  };
  return {
    document,
    selection: { active: { line: 0, character: PROOF_BODY } },
    async edit(apply) {
      apply({ replace({ start, end }, text) {
        document.text = document.text.slice(0, start.character) + text + document.text.slice(end.character);
        document.version += 1;
      } });
      return true;
    },
  };
}

function searchReply(request, generation, status = "found") {
  return {
    generation, status, version: 1, position: request.params.position,
    suggestions: status === "found" ? [{ text: "symm h", range: {
      start: { line: 0, character: PROOF_BODY }, end: { line: 0, character: PROOF_BODY + 4 },
    } }] : [],
  };
}

test("Find Proof previews source and applies it only after validating the selected revision", async (t) => {
  const h = createHarness();
  await h.extension.activate({ subscriptions: [] });
  t.after(() => h.extension.deactivate());
  const [client] = h.createdClients;
  const editor = searchEditor();
  h.setActiveEditor(editor);

  const search = h.registeredCommands.get("flick.searchProof")();
  const request = client.requests.at(-1);
  assert.equal(request.method, "flick/searchProof");
  assert.deepEqual(request.params, {
    textDocument: { uri: "file:///Proofs.fl" }, position: { line: 0, character: PROOF_BODY },
  });
  assert.equal(h.searches[0].options.cancellable, true);
  request.resolve(searchReply(request, 22));
  await turn();

  const [preview] = h.previews;
  assert.deepEqual(preview.items.map(({ label }) => label), ["symm h"]);
  assert.equal(editor.document.text, OPEN_PROOF);
  preview.select(0);
  await turn();
  const validate = client.requests.at(-1);
  assert.equal(validate.method, "flick/validateProofSearch");
  assert.deepEqual(validate.params, { textDocument: request.params.textDocument, generation: 22, version: 1 });
  assert.equal(editor.document.text, OPEN_PROOF);
  validate.resolve(true);
  await search;
  assert.equal(editor.document.text, OPEN_PROOF.replace("todo", "symm h"));
});

test("Find Proof cancels on request and discards revisions changed before insertion", async (t) => {
  const h = createHarness();
  await h.extension.activate({ subscriptions: [] });
  t.after(() => h.extension.deactivate());
  const [client] = h.createdClients;
  const editor = searchEditor();
  h.setActiveEditor(editor);
  const run = h.registeredCommands.get("flick.searchProof");

  const cancelled = run();
  const cancelledRequest = client.requests.at(-1);
  h.searches.at(-1).source.cancel();
  assert.equal(cancelledRequest.token.isCancellationRequested, true);
  cancelledRequest.resolve(searchReply(cancelledRequest, 2));
  await cancelled;
  assert.equal(h.previews.length, 0);

  const dependencyChanged = run();
  const request = client.requests.at(-1);
  request.resolve(searchReply(request, 2));
  await turn();
  const preview = h.previews.at(-1);
  client.notifications.get("flick/proofStateChanged")({ generation: 3 });
  assert.equal(preview.token.isCancellationRequested, true);
  preview.select(0);
  await dependencyChanged;
  assert.equal(client.requests.at(-1), request, "the superseded preview cannot request insertion");

  const stale = run();
  const staleRequest = client.requests.at(-1);
  staleRequest.resolve(searchReply(staleRequest, 3));
  await turn();
  h.previews.at(-1).select(0);
  await turn();
  client.requests.at(-1).resolve(false);
  await stale;
  assert.match(h.information.at(-1), /proof context changed/);

  const edited = run();
  const beforeEdit = client.requests.at(-1);
  beforeEdit.resolve(searchReply(beforeEdit, 4));
  await turn();
  h.previews.at(-1).select(0);
  await turn();
  h.changeDocument(editor.document);
  client.requests.at(-1).resolve(true);
  await edited;
  assert.equal(editor.document.text, OPEN_PROOF);
});

test("Find Proof distinguishes exhaustion from no checked candidate", async (t) => {
  const h = createHarness();
  await h.extension.activate({ subscriptions: [] });
  t.after(() => h.extension.deactivate());
  const [client] = h.createdClients;
  h.setActiveEditor(searchEditor());
  for (const status of ["no-candidate", "budget-exhausted"]) {
    const search = h.registeredCommands.get("flick.searchProof")();
    const request = client.requests.at(-1);
    request.resolve(searchReply(request, 1, status));
    await search;
  }
  assert.deepEqual(h.previews, []);
  assert.deepEqual(h.information, ["No checked proof term was found.", "Proof search reached its limit."]);
});
