// Client for `flick lsp --stdio`, managing lifecycle, configuration, output, and status bar.
const vscode = require("vscode");
const { LanguageClient, State } = require("vscode-languageclient/node");

let client;
let outputChannel;
let statusBar;
let stateDisposable;
let fileWatchers = [];
let lifecycleQueue = Promise.resolve();
let currentCommand = "flick";
let currentTrace = "off";
let activeState = State.Stopped;
let inFlightRequests = 0;
let hasStartError = false;

function enqueue(task) {
  const run = lifecycleQueue.then(task, task);
  lifecycleQueue = run.catch(() => {});
  return run;
}

function logInfo(message) {
  if (typeof outputChannel?.info === "function") {
    outputChannel.info(message);
  } else {
    outputChannel?.appendLine?.(message);
  }
}

function logError(message) {
  if (typeof outputChannel?.error === "function") {
    outputChannel.error(message);
  } else {
    outputChannel?.appendLine?.(message);
  }
}

function updateStatus(kind, detail) {
  if (!statusBar) {
    return;
  }
  const errorBg =
    typeof vscode.ThemeColor === "function"
      ? new vscode.ThemeColor("statusBarItem.errorBackground")
      : undefined;

  switch (kind) {
    case "starting":
      statusBar.text = "$(sync~spin) Flick: Starting";
      statusBar.tooltip = detail || `Starting Flick language server (${currentCommand})...`;
      statusBar.command = "flick.showOutput";
      statusBar.backgroundColor = undefined;
      break;
    case "ready":
      statusBar.text = "$(check) Flick: Ready";
      statusBar.tooltip = detail || `Flick language server is ready (${currentCommand}). Click to open output.`;
      statusBar.command = "flick.showOutput";
      statusBar.backgroundColor = undefined;
      break;
    case "busy":
      statusBar.text = "$(sync~spin) Flick: Busy";
      statusBar.tooltip = detail || "Flick language server is processing requests. Click to open output.";
      statusBar.command = "flick.showOutput";
      statusBar.backgroundColor = undefined;
      break;
    case "error":
      statusBar.text = "$(error) Flick: Error";
      statusBar.tooltip = detail || "Flick language server failed to start. Click to restart.";
      statusBar.command = "flick.restartServer";
      statusBar.backgroundColor = errorBg;
      break;
    default:
      statusBar.text = "$(circle-slash) Flick: Stopped";
      statusBar.tooltip = detail || "Flick language server is stopped. Click to restart.";
      statusBar.command = "flick.restartServer";
      statusBar.backgroundColor = undefined;
      break;
  }
}

function readConfig() {
  const config = vscode.workspace.getConfiguration("flick");
  return {
    command: config.get("path", "flick") || "flick",
    trace: config.get("trace.server", "off"),
  };
}

function disposeWatchers() {
  for (const watcher of fileWatchers) {
    watcher.dispose?.();
  }
  fileWatchers = [];
}

function createWatchers() {
  disposeWatchers();
  if (typeof vscode.workspace.createFileSystemWatcher === "function") {
    fileWatchers = [
      vscode.workspace.createFileSystemWatcher("**/*.fl"),
      vscode.workspace.createFileSystemWatcher("**/flick.toml"),
      vscode.workspace.createFileSystemWatcher("**/flick.lock"),
    ];
  }
  return fileWatchers;
}

async function stopCurrentClient() {
  stateDisposable?.dispose?.();
  stateDisposable = undefined;
  inFlightRequests = 0;

  const active = client;
  client = undefined;
  activeState = State.Stopped;

  if (!active) {
    disposeWatchers();
    return;
  }

  try {
    if (typeof active.dispose === "function") {
      await active.dispose();
    } else if (typeof active.stop === "function") {
      await active.stop();
    }
  } catch (err) {
    logError(`Failed to stop Flick language server: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    disposeWatchers();
  }
}

async function startNewClient() {
  const { command, trace } = readConfig();
  currentCommand = command;
  currentTrace = trace;
  inFlightRequests = 0;
  hasStartError = false;
  activeState = State.Starting;
  updateStatus("starting", `Starting Flick language server (${command})...`);
  logInfo(`Starting Flick language server: ${command} lsp --stdio`);

  const watchers = createWatchers();
  let instance;

  const clientOptions = {
    documentSelector: [
      { scheme: "file", language: "flick" },
      { scheme: "untitled", language: "flick" },
    ],
    outputChannel,
    traceOutputChannel: outputChannel,
    synchronize: {
      fileEvents: watchers,
    },
    middleware: {
      sendRequest(type, param, token, next) {
        if (instance !== client) {
          return next(type, param, token);
        }
        inFlightRequests += 1;
        if (activeState === State.Running && inFlightRequests === 1) {
          updateStatus("busy");
        }
        const finish = () => {
          if (instance !== client) {
            return;
          }
          inFlightRequests = Math.max(0, inFlightRequests - 1);
          if (activeState === State.Running && inFlightRequests === 0) {
            updateStatus("ready");
          }
        };
        return Promise.resolve(next(type, param, token)).then(
          (value) => {
            finish();
            return value;
          },
          (err) => {
            finish();
            throw err;
          },
        );
      },
    },
  };

  instance = new LanguageClient(
    "flick",
    "Flick",
    { command, args: ["lsp", "--stdio"] },
    clientOptions,
  );
  client = instance;

  if (typeof instance.onDidChangeState === "function") {
    stateDisposable = instance.onDidChangeState(({ newState }) => {
      if (instance !== client) {
        return;
      }
      activeState = newState;
      if (newState === State.Starting) {
        updateStatus("starting");
      } else if (newState === State.Running) {
        updateStatus(inFlightRequests > 0 ? "busy" : "ready");
      } else if (newState === State.StartFailed) {
        hasStartError = true;
        updateStatus("error", `Flick language server (${command}) failed to start. Click to restart.`);
      } else {
        inFlightRequests = 0;
        updateStatus(hasStartError ? "error" : "stopped");
      }
    });
  }

  try {
    await instance.start();
    if (instance === client && activeState === State.Starting) {
      activeState = State.Running;
      updateStatus(inFlightRequests > 0 ? "busy" : "ready");
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    logError(`Failed to start Flick language server (${command}): ${message}`);
    if (instance === client) {
      hasStartError = true;
      activeState = State.StartFailed;
      updateStatus("error", `Flick language server (${command}) failed: ${message}. Click to restart.`);
    }
  }
}

function restartServer() {
  return enqueue(async () => {
    logInfo("Restarting Flick language server...");
    await stopCurrentClient();
    await startNewClient();
  });
}

function activate(context) {
  outputChannel = vscode.window.createOutputChannel("Flick", { log: true });
  statusBar = vscode.window.createStatusBarItem("flick.statusBar", vscode.StatusBarAlignment.Left, 0);
  statusBar.show();

  const restartCommand = vscode.commands.registerCommand("flick.restartServer", () => restartServer());
  const showOutputCommand = vscode.commands.registerCommand("flick.showOutput", () => {
    outputChannel?.show?.(true);
  });

  const configListener = vscode.workspace.onDidChangeConfiguration((event) => {
    if (!event.affectsConfiguration("flick")) {
      return;
    }
    const { command, trace } = readConfig();
    if (command !== currentCommand || trace !== currentTrace) {
      logInfo(`Configuration changed (path=${command}, trace=${trace}); restarting language server...`);
      void restartServer();
    }
  });

  if (Array.isArray(context?.subscriptions)) {
    context.subscriptions.push(
      outputChannel,
      statusBar,
      restartCommand,
      showOutputCommand,
      configListener,
    );
  }

  return enqueue(() => startNewClient());
}

function deactivate() {
  return enqueue(async () => {
    await stopCurrentClient();
    statusBar?.dispose?.();
    statusBar = undefined;
    outputChannel?.dispose?.();
    outputChannel = undefined;
  });
}

module.exports = { activate, deactivate, restartServer };
