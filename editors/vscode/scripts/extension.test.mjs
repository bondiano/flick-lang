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

function createHarness({ initialConfig = {}, failStartOn = () => false } = {}) {
  const State = { Stopped: 1, Running: 2, Starting: 3, StartFailed: 4 };
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
      createdClients.push(this);
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
      if (failStartOn(this.serverOptions.command)) {
        this.emitState(State.StartFailed);
        throw new Error(`spawn ${this.serverOptions.command} ENOENT`);
      }
      this.emitState(State.Running);
    }
    async stop() {
      this.stopCalls += 1;
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
    window: {
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
      return { LanguageClient: FakeLanguageClient, State };
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

test("package.json contributes restartServer, showOutput, configuration, and grammar", () => {
  const commands = manifest.contributes?.commands ?? [];
  const restart = commands.find((c) => c.command === "flick.restartServer");
  assert.ok(restart, "flick.restartServer command must be contributed");
  assert.equal(restart.title, "Restart Language Server");
  assert.equal(restart.category, "Flick");

  const showOutput = commands.find((c) => c.command === "flick.showOutput");
  assert.ok(showOutput, "flick.showOutput command must be contributed");

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

  // Fixing flick.path and triggering flick.restartServer recovers cleanly to Ready.
  h.configValues.path = "/usr/local/bin/flick";
  await h.registeredCommands.get("flick.restartServer")();
  assert.equal(h.createdClients.length, 2);
  assert.equal(h.createdClients[0].disposeCalls, 1);
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
