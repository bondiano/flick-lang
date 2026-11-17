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
