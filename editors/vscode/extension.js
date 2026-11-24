// Client for `flick lsp --stdio`, managing lifecycle, configuration, output, and status bar.
const vscode = require("vscode");
const { LanguageClient, State, CloseAction, ErrorAction } = require("vscode-languageclient/node");
const { createProofView } = require("./proof-view.js");
const { createProofSearch } = require("./proof-search.js");

const DEFAULT_START_TIMEOUT_MS = 15_000;

let client;
let outputChannel;
let statusBar;
let stateDisposable;
let proofStateDisposable;
let proofView;
let proofSearch;
let extensionDisposables = [];
let fileWatchers = [];
let lifecycleQueue = Promise.resolve();
let lifecycleGeneration = 0;
let isDeactivated = false;
let inFlightStart = null;
let currentCommand = "flick";
let currentTrace = "off";
let activeState = State.Stopped;
let inFlightRequests = 0;
let hasStartError = false;

class StartAbortedError extends Error {
  constructor(message) {
    super(message);
    this.name = "StartAbortedError";
  }
}

function getStartTimeoutMs() {
  const raw = Number(process.env.FLICK_LSP_START_TIMEOUT_MS);
  return Number.isFinite(raw) && raw > 0 ? raw : DEFAULT_START_TIMEOUT_MS;
}

function terminateServerProcess(instance) {
  if (!instance) {
    return;
  }
  const proc = instance.serverProcess ?? instance._serverProcess;
  if (proc && !proc.killed) {
    try {
      proc.kill("SIGKILL");
    } catch {}
  }
  try {
    instance._connection?.dispose?.();
  } catch {}
}

function abortInFlightStart(reason) {
  const pending = inFlightStart;
  inFlightStart = null;
  pending?.abort(reason);
}

function disposeExtensionDisposables() {
  for (const item of extensionDisposables) {
    item?.dispose?.();
  }
  extensionDisposables = [];
}

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

function proofConnectionChanged() {
  proofView?.connectionChanged();
  proofSearch?.connectionChanged();
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
  proofStateDisposable?.dispose();
  proofStateDisposable = undefined;
  inFlightRequests = 0;

  const active = client;
  client = undefined;
  activeState = State.Stopped;
  proofConnectionChanged();

  if (!active) {
    disposeWatchers();
    return;
  }

  try {
    if (active.state === State.Running) {
      if (typeof active.dispose === "function") {
        await active.dispose();
      } else if (typeof active.stop === "function") {
        await active.stop();
      }
    } else {
      terminateServerProcess(active);
    }
  } catch (err) {
    logError(`Failed to stop Flick language server: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    disposeWatchers();
  }
}

async function startNewClient(expectedGeneration) {
  if (isDeactivated || (expectedGeneration !== undefined && expectedGeneration !== lifecycleGeneration)) {
    return;
  }

  const { command, trace } = readConfig();
  currentCommand = command;
  currentTrace = trace;
  inFlightRequests = 0;
  hasStartError = false;
  activeState = State.Starting;
  updateStatus("starting", `Starting Flick language server (${command})...`);
  logInfo(`Starting Flick language server: ${command} lsp --stdio`);

  let instance;
  let aborted = false;
  let defaultErrorHandler;

  try {
    const watchers = createWatchers();
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
      errorHandler: {
        error(error, message, count) {
          if (aborted || isDeactivated || instance !== client) {
            return { action: ErrorAction?.Shutdown ?? 2, handled: true };
          }
          return defaultErrorHandler
            ? defaultErrorHandler.error(error, message, count)
            : { action: count && count <= 3 ? (ErrorAction?.Continue ?? 1) : (ErrorAction?.Shutdown ?? 2) };
        },
        closed() {
          if (
            aborted ||
            isDeactivated ||
            instance !== client ||
            activeState === State.Starting ||
            activeState === State.StartFailed
          ) {
            return { action: CloseAction?.DoNotRestart ?? 1, handled: true };
          }
          return defaultErrorHandler
            ? defaultErrorHandler.closed()
            : { action: CloseAction?.DoNotRestart ?? 1 };
        },
      },
      middleware: {
        async sendRequest(type, param, token, next) {
          if (instance !== client) {
            return next(type, param, token);
          }
          inFlightRequests += 1;
          if (activeState === State.Running && inFlightRequests === 1) {
            updateStatus("busy");
          }
          try {
            return await next(type, param, token);
          } finally {
            if (instance === client) {
              inFlightRequests = Math.max(0, inFlightRequests - 1);
              if (activeState === State.Running && inFlightRequests === 0) {
                updateStatus("ready");
              }
            }
          }
        },
      },
    };

    instance = new LanguageClient(
      "flick",
      "Flick",
      { command, args: ["lsp", "--stdio"] },
      clientOptions,
    );
    defaultErrorHandler =
      typeof instance.createDefaultErrorHandler === "function"
        ? instance.createDefaultErrorHandler()
        : undefined;

    if (typeof instance.createMessageTransports === "function") {
      const origCreateTransports = instance.createMessageTransports.bind(instance);
      instance.createMessageTransports = async (...args) => {
        if (aborted || isDeactivated || instance !== client) {
          throw new StartAbortedError("Language server start aborted before spawn");
        }
        const transports = await origCreateTransports(...args);
        if (aborted || isDeactivated || instance !== client) {
          terminateServerProcess(instance);
          transports?.reader?.dispose?.();
          transports?.writer?.dispose?.();
          throw new StartAbortedError("Language server start aborted");
        }
        return transports;
      };
    }

    client = instance;
    proofStateDisposable = instance.onNotification("flick/proofStateChanged", (event) => {
      if (instance === client && !aborted && !isDeactivated) {
        proofView?.snapshotChanged(event);
        proofSearch?.snapshotChanged(event);
      }
    });

    if (typeof instance.onDidChangeState === "function") {
      stateDisposable = instance.onDidChangeState(({ newState }) => {
        if (instance !== client || aborted || isDeactivated) {
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
          if (!hasStartError) {
            updateStatus("stopped");
          }
        }
        proofConnectionChanged();
      });
    }

    let timeoutId;
    let settled = false;
    let abortReject;
    const abortPromise = new Promise((_, reject) => {
      abortReject = reject;
    });

    const cancelStart = (err) => {
      if (settled) {
        return;
      }
      settled = true;
      aborted = true;
      clearTimeout(timeoutId);
      terminateServerProcess(instance);
      abortReject(err);
    };

    const entry = {
      instance,
      abort: (reason) => cancelStart(new StartAbortedError(reason || "Language server start aborted")),
    };
    inFlightStart = entry;

    const timeoutMs = getStartTimeoutMs();
    timeoutId = setTimeout(() => {
      cancelStart(new Error(`Timed out after ${timeoutMs}ms waiting for language server to initialize`));
    }, timeoutMs);

    const startPromise = Promise.resolve(instance.start());
    startPromise.then(
      () => {
        if (aborted || isDeactivated || instance !== client) {
          terminateServerProcess(instance);
          if (instance.state === State.Running && typeof instance.dispose === "function") {
            instance.dispose().catch(() => {});
          }
        }
      },
      () => {},
    );

    try {
      await Promise.race([startPromise, abortPromise]);
    } finally {
      settled = true;
      clearTimeout(timeoutId);
      if (inFlightStart === entry) {
        inFlightStart = null;
      }
    }

    if (
      aborted ||
      isDeactivated ||
      (expectedGeneration !== undefined && expectedGeneration !== lifecycleGeneration) ||
      instance !== client
    ) {
      terminateServerProcess(instance);
      if (instance.state === State.Running && typeof instance.dispose === "function") {
        await instance.dispose().catch(() => {});
      }
      return;
    }

    if (activeState === State.Starting) {
      activeState = State.Running;
      updateStatus(inFlightRequests > 0 ? "busy" : "ready");
      proofConnectionChanged();
    }
  } catch (err) {
    if (err instanceof StartAbortedError) {
      if (instance === client) {
        stateDisposable?.dispose?.();
        stateDisposable = undefined;
        proofStateDisposable?.dispose();
        proofStateDisposable = undefined;
        client = undefined;
        activeState = State.Stopped;
        proofConnectionChanged();
        disposeWatchers();
      }
      return;
    }
    const message = err instanceof Error ? err.message : String(err);
    logError(`Failed to start Flick language server (${command}): ${message}`);
    if (!instance) {
      disposeWatchers();
    }
    if (!isDeactivated && (instance === client || instance === undefined)) {
      hasStartError = true;
      activeState = State.StartFailed;
      updateStatus("error", `Flick language server (${command}) failed: ${message}. Click to restart.`);
      proofConnectionChanged();
    }
  }
}

function restartServer() {
  if (isDeactivated) {
    return Promise.resolve();
  }
  lifecycleGeneration += 1;
  const generation = lifecycleGeneration;
  abortInFlightStart("Restarting Flick language server");
  return enqueue(async () => {
    if (isDeactivated || generation !== lifecycleGeneration) {
      return;
    }
    logInfo("Restarting Flick language server...");
    await stopCurrentClient();
    if (isDeactivated || generation !== lifecycleGeneration) {
      return;
    }
    await startNewClient(generation);
  });
}

function activate(context) {
  isDeactivated = false;
  lifecycleGeneration += 1;
  const generation = lifecycleGeneration;

  outputChannel = vscode.window.createOutputChannel("Flick", { log: true });
  statusBar = vscode.window.createStatusBarItem("flick.statusBar", vscode.StatusBarAlignment.Left, 0);
  statusBar.show();
  const readyClient = () => activeState === State.Running ? client : undefined;
  proofView = createProofView(vscode, readyClient, logError);
  proofSearch = createProofSearch(vscode, readyClient, logError);

  const restartCommand = vscode.commands.registerCommand("flick.restartServer", () => restartServer());
  const showOutputCommand = vscode.commands.registerCommand("flick.showOutput", () => {
    outputChannel?.show?.(true);
  });
  const searchProofCommand = vscode.commands.registerCommand("flick.searchProof", proofSearch.run);

  const configListener = vscode.workspace.onDidChangeConfiguration((event) => {
    if (isDeactivated || !event.affectsConfiguration("flick")) {
      return;
    }
    const { command, trace } = readConfig();
    if (command !== currentCommand || trace !== currentTrace) {
      logInfo(`Configuration changed (path=${command}, trace=${trace}); restarting language server...`);
      restartServer().catch((err) => {
        logError(`Failed to restart Flick language server: ${err instanceof Error ? err.message : String(err)}`);
      });
    }
  });

  extensionDisposables = [restartCommand, showOutputCommand, searchProofCommand, configListener];
  if (Array.isArray(context?.subscriptions)) {
    context.subscriptions.push(
      outputChannel,
      statusBar,
      restartCommand,
      showOutputCommand,
      searchProofCommand,
      configListener,
      proofView,
      proofSearch,
    );
  }

  return enqueue(() => startNewClient(generation));
}

function deactivate() {
  isDeactivated = true;
  lifecycleGeneration += 1;
  abortInFlightStart("Deactivating Flick extension");
  disposeExtensionDisposables();
  proofView?.dispose();
  proofView = undefined;
  proofSearch?.dispose();
  proofSearch = undefined;
  return enqueue(async () => {
    await stopCurrentClient();
    statusBar?.dispose?.();
    statusBar = undefined;
    outputChannel?.dispose?.();
    outputChannel = undefined;
  });
}

module.exports = { activate, deactivate, restartServer };
