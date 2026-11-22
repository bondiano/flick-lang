// Runtime bootstrap, Node 24 resolver hooks, IPC transport, and per-execution
// handle/owner state shared across the persistent host/FFI bridge worker.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { registerHooks } from "node:module";
import { Worker } from "node:worker_threads";

import { initWellKnownTags } from "./wire.mjs";

export const initialFetch = globalThis.fetch;
export const initialConsoleError = console.error;
export const initialArgv1 = process.argv[1];

const port = Number(process.argv[2]);
export const repoRoot = process.argv[3];
process.argv.splice(2);

const bindingsPath = path.join(repoRoot, "packages/effect-ts/compiler-bindings.json");
const bindings = JSON.parse(fs.readFileSync(bindingsPath, "utf8"));
export const routeMap = new Map(bindings.routes);
export const bridgeOverrides = new Set(bindings.bridge_overrides ?? []);
const runtimePkgJson = JSON.parse(
  fs.readFileSync(path.join(repoRoot, "runtime/package.json"), "utf8"),
);
const effectTsPkgJson = JSON.parse(
  fs.readFileSync(path.join(repoRoot, "packages/effect-ts/package.json"), "utf8"),
);
const runtimePkgUrl = pathToFileURL(path.join(repoRoot, "runtime/package.json")).href;
const effectTsPkgUrl = pathToFileURL(path.join(repoRoot, "packages/effect-ts/package.json")).href;

function resolvePkgExport(pkgRoot, exportsMap, subpath) {
  const rel = subpath === "" ? "." : `./${subpath}`;
  const exact = exportsMap[rel];
  if (exact) {
    const target = typeof exact === "string" ? exact : exact.default;
    return pathToFileURL(path.join(pkgRoot, target)).href;
  }
  for (const [pattern, val] of Object.entries(exportsMap)) {
    const star = pattern.indexOf("*");
    if (star < 0) continue;
    const prefix = pattern.slice(0, star);
    const suffix = pattern.slice(star + 1);
    if (rel.startsWith(prefix) && rel.endsWith(suffix)) {
      const matched = rel.slice(prefix.length, rel.length - suffix.length);
      const target = (typeof val === "string" ? val : val.default).replace("*", matched);
      const direct = path.join(pkgRoot, target);
      if (fs.existsSync(direct)) return pathToFileURL(direct).href;
      const asIndex = direct.replace(/\.js$/, "/index.js");
      if (fs.existsSync(asIndex)) return pathToFileURL(asIndex).href;
      return pathToFileURL(direct).href;
    }
  }
  return null;
}

export function effectSpecifier(spec) {
  for (const [prefix, target, isPrefix] of bindings.aliases) {
    if (isPrefix && spec.startsWith(prefix)) {
      return target + spec.slice(prefix.length);
    }
    if (!isPrefix && spec === prefix) {
      return target;
    }
  }
  return spec;
}

function resolveFlickSpec(spec) {
  if (spec.startsWith("flick:")) {
    const routed = routeMap.get(spec) ?? effectSpecifier(spec);
    return resolveFlickSpec(routed);
  }
  if (spec === "@flick-lang/runtime") {
    return resolvePkgExport(path.join(repoRoot, "runtime"), runtimePkgJson.exports, "");
  }
  if (spec.startsWith("@flick-lang/runtime/")) {
    return resolvePkgExport(
      path.join(repoRoot, "runtime"),
      runtimePkgJson.exports,
      spec.slice("@flick-lang/runtime/".length),
    );
  }
  if (spec === "@flick-lang/effect-ts") {
    return resolvePkgExport(
      path.join(repoRoot, "packages/effect-ts"),
      effectTsPkgJson.exports,
      "",
    );
  }
  if (spec.startsWith("@flick-lang/effect-ts/")) {
    return resolvePkgExport(
      path.join(repoRoot, "packages/effect-ts"),
      effectTsPkgJson.exports,
      spec.slice("@flick-lang/effect-ts/".length),
    );
  }
  return null;
}

registerHooks({
  resolve(specifier, context, nextResolve) {
    const mapped = resolveFlickSpec(specifier);
    if (mapped !== null) {
      return { url: mapped, shortCircuit: true };
    }
    if ((specifier.startsWith("./") || specifier.startsWith("../")) && context.parentURL) {
      try {
        const parentUrl = new URL(context.parentURL);
        const execParam = parentUrl.searchParams.get("exec");
        if (execParam !== null) {
          const resolved = nextResolve(specifier, context);
          if (resolved.url.startsWith("file:") && !resolved.url.includes("?exec=")) {
            const targetPath = fileURLToPath(resolved.url);
            if (
              !targetPath.startsWith(path.join(repoRoot, "runtime")) &&
              !targetPath.startsWith(path.join(repoRoot, "packages")) &&
              !targetPath.includes(`${path.sep}node_modules${path.sep}`)
            ) {
              return { ...resolved, url: `${resolved.url}?exec=${execParam}` };
            }
          }
          return resolved;
        }
      } catch {
        // Fall through to standard resolution below.
      }
    }
    try {
      return nextResolve(specifier, context);
    } catch (err) {
      if (
        !specifier.startsWith(".") &&
        !specifier.startsWith("/") &&
        !specifier.startsWith("file:") &&
        !specifier.startsWith("node:")
      ) {
        try {
          return nextResolve(specifier, { ...context, parentURL: effectTsPkgUrl });
        } catch {
          try {
            return nextResolve(specifier, { ...context, parentURL: runtimePkgUrl });
          } catch {
            throw err;
          }
        }
      }
      throw err;
    }
  },
});

// Load Flick runtime and Effect bridge once per worker process.
export const runtime = await import("@flick-lang/runtime/rt");
export const stubsMod = await import(
  pathToFileURL(path.join(repoRoot, "runtime/dist/core/stubs.js")).href
);
export const tagsMod = await import(
  pathToFileURL(path.join(repoRoot, "runtime/dist/tags.js")).href
);
export const causeMod = await import("@flick-lang/runtime/rt/cause");
export const fiberMod = await import("@flick-lang/runtime/rt/fiber");
export const ownerMod = await import("@flick-lang/runtime/rt/owner");
export const layerMod = await import("@flick-lang/runtime/rt/layer");
export const durationMod = await import("@flick-lang/runtime/rt/duration");
export const outcomeMod = await import("@flick-lang/runtime/rt/outcome");
export const schemaMod = await import("@flick-lang/runtime/rt/schema");
export const codecMod = await import("@flick-lang/runtime/rt/codec");
export const routeMod = await import("@flick-lang/runtime/rt/route");
const runnerHostMod = await import(
  pathToFileURL(path.join(repoRoot, "runtime/dist/rt/runner/host.js")).href
);
const effRunnerHostMod = await import(
  pathToFileURL(path.join(repoRoot, "packages/effect-ts/dist/runner/host.js")).href
);
export const effEffectsMod = await import(
  pathToFileURL(path.join(repoRoot, "packages/effect-ts/dist/effects.js")).href
);
export const syncMod = await import(
  pathToFileURL(path.join(repoRoot, "packages/effect-ts/dist/sync.js")).href
);
export const bridge = await import("@flick-lang/effect-ts/bridge");
export const {
  Context: EContext,
  DateTime: EDateTime,
  Duration: EDuration,
  Effect,
  Equal: EEqual,
  Option: EOption,
  Redacted: ERedacted,
  Result: EResult,
  Schema: ESchema,
} = await import("effect");

export const fnStages = new WeakMap();
export const fnResultHint = new WeakMap();
export const hostFnUnwrap = new WeakMap();

bridge.initializeBridge();
initWellKnownTags(tagsMod.tag);

// Base host services so Effect/RT code reading `Host.*` sees live Node services
// unless overridden by an active Rust `provide` layer.
export const rootHostServices = runnerHostMod.hostLiveServices;
export const rootEffectHostServices = [
  effRunnerHostMod.hostConsole,
  effRunnerHostMod.hostClock,
  effRunnerHostMod.hostRandom,
  effRunnerHostMod.hostEnvironment,
  effRunnerHostMod.hostArguments,
  effRunnerHostMod.hostProcess,
];

export const wireCtx = {
  runtime,
  tagFn: tagsMod.tag,
  cause: causeMod,
  Failure: outcomeMod.Failure,
  Duration: durationMod.Duration,
  EDateTime,
  EDuration,
  EOption,
  EResult,
  ESchema,
  Redacted: ERedacted,
  layerMod,
  schemaMod,
  codecMod,
  routeMod,
  bridge,
  layerEmpty: layerMod.constant([]),
  makeCallback: null,
  makeTask: null,
  makeCellProxy: null,
};

export const workerState = {
  syncDepth: 0,
  dispatchSyncNested: null,
  handleMessage: null,
};

// Spawn dedicated TCP I/O thread + SharedArrayBuffer for re-entrant sync callbacks.
const ctrlBuf = new SharedArrayBuffer(16);
const dataBuf = new SharedArrayBuffer(1024 * 1024);
export const ctrl = new Int32Array(ctrlBuf);
const dataView = new Uint8Array(dataBuf);
export const ioWorker = new Worker(new URL("./io.mjs", import.meta.url), {
  workerData: { port, ctrlBuf, dataBuf },
});

export function send(obj) {
  ioWorker.postMessage(JSON.stringify(obj));
}

// Capture process.stdout.write into ordered IPC stdout events.
process.stdout.write = (chunk, encoding, cb) => {
  const text = Buffer.isBuffer(chunk)
    ? chunk.toString(typeof encoding === "string" ? encoding : "utf8")
    : String(chunk);
  if (text.length > 0) {
    send({ type: "stdout", text });
  }
  const done = typeof encoding === "function" ? encoding : cb;
  if (typeof done === "function") done();
  return true;
};

export class ExecState {
  constructor(id) {
    this.id = id;
    this.handles = new Map();
    this.objToId = new Map();
    this.nextHandle = 1;
    this.callbacks = new Map();
    this.owners = new Map();
    this.ownerToId = new WeakMap();
    this.pendingCalls = new Map();
    this.pendingCbs = new Map();
    this.nextCbReq = 1;
    this.moduleCache = new Map();
    this.intAtoms = new WeakSet();
  }

  retainHandle(value) {
    if (value !== null && (typeof value === "object" || typeof value === "function")) {
      const existing = this.objToId.get(value);
      if (existing !== undefined) {
        const entry = this.handles.get(existing);
        if (entry !== undefined) {
          entry.refCount += 1;
          return existing;
        }
      }
    }
    const id = this.nextHandle++;
    this.handles.set(id, { value, refCount: 1 });
    if (value !== null && (typeof value === "object" || typeof value === "function")) {
      this.objToId.set(value, id);
    }
    return id;
  }

  releaseHandles(ids) {
    for (const id of ids) {
      const entry = this.handles.get(id);
      if (entry === undefined) continue;
      entry.refCount -= 1;
      if (entry.refCount <= 0) {
        this.handles.delete(id);
        if (
          entry.value !== null &&
          (typeof entry.value === "object" || typeof entry.value === "function")
        ) {
          if (this.objToId.get(entry.value) === id) {
            this.objToId.delete(entry.value);
          }
        }
      }
    }
  }

  getOwner(ownerId) {
    let owner = this.owners.get(ownerId);
    if (owner === undefined) {
      owner = new ownerMod.Owner();
      this.owners.set(ownerId, owner);
      this.ownerToId.set(owner, ownerId);
    }
    return owner;
  }

  ownerIdOf(obj) {
    return this.ownerToId.get(obj);
  }
}

export const executions = new Map();
export const builtinModuleCache = new Map();

const reactContextModuleUrls = [
  pathToFileURL(path.join(repoRoot, "runtime/dist/rt/ui/render.js")).href,
  pathToFileURL(path.join(repoRoot, "packages/effect-ts/dist/ui/react/render.js")).href,
];
let loadedReactContexts = null;

export async function resetReactContexts() {
  let hasUi = Boolean(globalThis.document);
  if (!hasUi) {
    for (const key of builtinModuleCache.keys()) {
      if (key.includes("/ui") || key.includes("ui/")) {
        hasUi = true;
        break;
      }
    }
  }
  if (!hasUi) return;
  if (loadedReactContexts === null) {
    loadedReactContexts = [];
    for (const url of reactContextModuleUrls) {
      try {
        const mod = await import(url);
        for (const name of ["RegistryContext", "StreamContext", "BridgeContext"]) {
          if (mod[name] && typeof mod[name] === "object") {
            loadedReactContexts.push(mod[name]);
          }
        }
      } catch {
        // Ignore if UI render module is unavailable.
      }
    }
  }
  for (const ctx of loadedReactContexts) {
    ctx._currentRenderer = null;
    ctx._currentRenderer2 = null;
  }
}

export function readSyncLine() {
  const chunks = [];
  // Transition 0/4 -> 1 without clobbering 3 (closed), then wake ioWorker.
  const cur = Atomics.load(ctrl, 0);
  if (cur === 3) return null;
  if (cur === 0 || cur === 4) {
    Atomics.compareExchange(ctrl, 0, cur, 1);
    ioWorker.postMessage({ wake: true });
  }
  for (;;) {
    let st = Atomics.load(ctrl, 0);
    while (st === 1) {
      Atomics.wait(ctrl, 0, 1);
      st = Atomics.load(ctrl, 0);
    }
    if (st === 3) return null;
    const len = Atomics.load(ctrl, 1);
    const more = Atomics.load(ctrl, 2);
    chunks.push(Buffer.from(dataView.subarray(0, len)));
    // Keep 1 if more chunks belong to this line; otherwise pause ioWorker at 4.
    Atomics.store(ctrl, 0, more === 1 ? 1 : 4);
    Atomics.notify(ctrl, 0, 1);
    if (more === 0) {
      return Buffer.concat(chunks).toString("utf8");
    }
  }
}

export function isTrustedPackage(from) {
  if (!from) return false;
  return (
    from.startsWith("flick:") ||
    from === "@flick-lang/runtime" ||
    from.startsWith("@flick-lang/runtime/") ||
    from === "@flick-lang/effect-ts" ||
    from.startsWith("@flick-lang/effect-ts/")
  );
}

export function containsEff(shape) {
  if (!shape || typeof shape === "string") return false;
  if ("Eff" in shape) return true;
  if ("Fun" in shape) {
    return shape.Fun.args.some(containsEff) || containsEff(shape.Fun.result);
  }
  if ("List" in shape) return containsEff(shape.List);
  if ("Option" in shape) return containsEff(shape.Option);
  if ("Result" in shape) return containsEff(shape.Result[0]) || containsEff(shape.Result[1]);
  if ("Tuple" in shape) return shape.Tuple.some(containsEff);
  if ("Record" in shape) return shape.Record.some(([, s]) => containsEff(s));
  return false;
}
