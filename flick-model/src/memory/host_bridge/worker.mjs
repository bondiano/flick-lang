// Persistent Node 24 worker entrypoint for the counted Rust <-> Host/FFI bridge.
// Wires runtime bootstrap, callback proxies, value adapters, and message handlers.
import { Readable } from "node:stream";

import { decodeWire, registerTag } from "./wire.mjs";
import {
  buildCallCtx,
  finishCallErr,
  finishCallOk,
  invokeTarget,
  prepareArgs,
  resolveTargetSync,
} from "./worker_adapt.mjs";
import { decodeFailure, makeCallback, makeCellProxy, makeTask } from "./worker_callbacks.mjs";
import {
  handleAwaitHandle,
  handleBuildLayer,
  handleCall,
  handleCallHandle,
  handleCloseOwner,
  handleGetField,
  handleLookupService,
  handlePerformService,
} from "./worker_handlers.mjs";
import {
  EEqual,
  Effect,
  ExecState,
  bridge,
  builtinModuleCache,
  causeMod,
  executions,
  fiberMod,
  initialArgv1,
  initialConsoleError,
  initialFetch,
  ioWorker,
  ownerMod,
  resetReactContexts,
  runtime,
  send,
  tagsMod,
  wireCtx,
  workerState,
} from "./worker_runtime.mjs";

wireCtx.makeCallback = makeCallback;
wireCtx.makeTask = makeTask;
wireCtx.makeCellProxy = makeCellProxy;

function dispatchSyncNested(msg) {
  if (msg.op === "release_handles") {
    executions.get(msg.exec)?.releaseHandles(msg.ids);
    return;
  }
  if (msg.op === "call") {
    // Synchronous nested call while already inside a sync callback.
    // If async or module not yet cached, immediately return `pending` so a
    // sync_task can yield cleanly to async without deadlocking Atomics.wait.
    const exec = executions.get(msg.exec);
    const spec = msg.spec;
    const callId = msg.call_id;
    try {
      const resolved = resolveTargetSync(exec, spec);
      if (resolved.needsAsyncLoad || spec.mode === "Async") {
        exec.pendingCalls.set(callId, { abort: () => {} });
        send({ type: "pending", call_id: callId });
        queueMicrotask(() => {
          if (!exec.pendingCalls.has(callId) || exec.closing) return;
          handleCall(msg, true).catch((err) => {
            if (!exec.pendingCalls.delete(callId) || exec.closing) return;
            finishCallErr(exec, callId, err, spec.has_catch);
          });
        });
        return;
      }
      const { target, bridged } = resolved;
      const finalArgs = prepareArgs(exec, spec, msg.args, bridged, null);
      const callCtx = buildCallCtx(exec, msg);
      if (spec.mode === "Effect") {
        const made = fiberMod.inCtx(callCtx, () => invokeTarget(spec, target, finalArgs));
        const runOrVal = Effect.isEffect(made)
          ? fiberMod.inCtx(callCtx, () => bridge.fromEffect(made))
          : made;
        if (fiberMod.isRun(runOrVal) || runOrVal instanceof fiberMod.Instruction) {
          const fiber = fiberMod.forkNow(() => runOrVal, callCtx);
          if (fiber.exit !== undefined) {
            if (fiber.exit.cause === undefined) {
              finishCallOk(exec, callId, spec, bridged, fiber.exit.value);
            } else {
              finishCallErr(exec, callId, new causeMod.Raise(fiber.exit.cause), false);
            }
            return;
          }
          exec.pendingCalls.set(callId, { abort: () => fiber.interrupt(0) });
          send({ type: "pending", call_id: callId });
          fiber.observe((exit) => {
            if (!exec.pendingCalls.delete(callId) || exec.closing) return;
            if (exit.cause === undefined) {
              try {
                finishCallOk(exec, callId, spec, bridged, exit.value);
              } catch (err) {
                finishCallErr(exec, callId, err, false);
              }
            } else {
              finishCallErr(exec, callId, new causeMod.Raise(exit.cause), false);
            }
          });
          return;
        }
        finishCallOk(exec, callId, spec, bridged, runOrVal);
        return;
      }
      const raw = fiberMod.inCtx(callCtx, () =>
        bridged
          ? bridge.inSyncContext(() => invokeTarget(spec, target, finalArgs))
          : invokeTarget(spec, target, finalArgs),
      );
      finishCallOk(exec, callId, spec, bridged, raw);
    } catch (err) {
      finishCallErr(exec, callId, err, spec.has_catch);
    }
    return;
  }
  if (msg.op === "call_handle") {
    handleCallHandle(msg);
    return;
  }
  if (msg.op === "await_handle") {
    handleAwaitHandle(msg);
    return;
  }
  if (msg.op === "cb_return") {
    const exec = executions.get(msg.exec);
    const pending = exec?.pendingCbs.get(msg.req_id);
    if (pending) {
      exec.pendingCbs.delete(msg.req_id);
      if (msg.ok) {
        let val = decodeWire(msg.value, exec, wireCtx);
        if (pending.retCopy) val = runtime.copyData(val);
        pending.resolve(val);
      } else {
        pending.reject(decodeFailure(msg, exec));
      }
    }
    return;
  }
  if (msg.op === "get_field") {
    handleGetField(msg);
    return;
  }
  if (msg.op === "close_owner") {
    handleCloseOwner(msg);
    return;
  }
  if (msg.op === "build_layer") {
    handleBuildLayer(msg);
    return;
  }
  if (msg.op === "lookup_service") {
    handleLookupService(msg);
    return;
  }
  if (msg.op === "perform_service") {
    handlePerformService(msg);
    return;
  }
  if (msg.op === "equals") {
    const exec = executions.get(msg.exec);
    const a = exec?.handles.get(msg.a);
    const b = exec?.handles.get(msg.b);
    const equal = a !== undefined && b !== undefined && EEqual.equals(a.value, b.value);
    send({ type: "equals_done", equal });
    return;
  }
  if (msg.op === "cancel") {
    const exec = executions.get(msg.exec);
    const pending = exec?.pendingCalls.get(msg.call_id);
    if (pending) {
      exec.pendingCalls.delete(msg.call_id);
      pending.abort();
    }
  }
}

async function handleMessage(line) {
  if (line === null) {
    process.exit(0);
  }
  const msg = JSON.parse(line);
  switch (msg.op) {
    case "open_exec": {
      if (process.stdin.readableEnded || process.stdin.destroyed) {
        const freshStdin = new Readable({
          read() {
            this.push(null);
          },
        });
        Object.defineProperty(process, "stdin", {
          value: freshStdin,
          configurable: true,
          writable: true,
        });
      }
      executions.set(msg.exec, new ExecState(msg.exec));
      if (Array.isArray(msg.tags)) {
        for (const fullTag of msg.tags) {
          registerTag(fullTag, tagsMod.tag);
        }
      }
      if (Array.isArray(msg.imports)) {
        for (const spec of msg.imports) {
          if (!builtinModuleCache.has(spec)) {
            try {
              builtinModuleCache.set(spec, await import(spec));
            } catch {
              // Fall back to lazy resolution if specifier is non-standard.
            }
          }
        }
      }
      send({ type: "ok" });
      break;
    }
    case "close_exec": {
      const exec = executions.get(msg.exec);
      if (exec) {
        exec.closing = true;
        for (const pending of exec.pendingCbs.values()) {
          pending.reject(new Error("execution closed"));
        }
        exec.pendingCbs.clear();
        for (const pending of exec.pendingCalls.values()) {
          pending.abort();
        }
        exec.pendingCalls.clear();
        const closeWaiters = [];
        for (const owner of exec.owners.values()) {
          if (owner.open) {
            const fiber = fiberMod.forkNow(
              () => ownerMod.close(owner, causeMod.succeeded(undefined)),
              fiberMod.currentCtx(),
            );
            if (fiber.exit === undefined) {
              closeWaiters.push(new Promise((res) => fiber.observe(res)));
            }
          }
        }
        if (closeWaiters.length > 0) {
          await Promise.race([
            Promise.all(closeWaiters),
            new Promise((res) => setTimeout(res, 300)),
          ]);
        }
        exec.owners.clear();
        const liveHandles = exec.handles.size;
        exec.handles.clear();
        exec.objToId.clear();
        exec.callbacks.clear();
        exec.pendingCbs.clear();
        executions.delete(msg.exec);
        if (globalThis.document?.body) {
          globalThis.document.body.innerHTML = "";
        }
        await resetReactContexts();
        globalThis.fetch = initialFetch;
        console.error = initialConsoleError;
        process.argv[1] = initialArgv1;
        send({ type: "closed", live_handles: liveHandles });
      } else {
        if (globalThis.document?.body) {
          globalThis.document.body.innerHTML = "";
        }
        await resetReactContexts();
        globalThis.fetch = initialFetch;
        console.error = initialConsoleError;
        process.argv[1] = initialArgv1;
        send({ type: "closed", live_handles: 0 });
      }
      break;
    }
    case "set_entry_script": {
      process.argv[1] = typeof msg.path === "string" ? msg.path : initialArgv1;
      send({ type: "ok" });
      break;
    }
    case "set_fetch_mock": {
      const replies = msg.replies;
      if (replies && typeof replies === "object") {
        globalThis.fetch = (input) => {
          const url = input instanceof Request ? input.url : String(input);
          if (!Object.hasOwn(replies, url)) {
            return Promise.reject(new Error(`No fetch fixture for ${url}`));
          }
          return Promise.resolve(Response.json(replies[url]));
        };
      } else {
        globalThis.fetch = initialFetch;
      }
      send({ type: "ok" });
      break;
    }
    case "release_handles":
      executions.get(msg.exec)?.releaseHandles(msg.ids);
      break;
    case "close_owner":
      handleCloseOwner(msg);
      break;
    case "call":
      await handleCall(msg);
      break;
    case "call_handle":
      handleCallHandle(msg);
      break;
    case "await_handle":
      handleAwaitHandle(msg);
      break;
    case "get_field":
      handleGetField(msg);
      break;
    case "build_layer":
      handleBuildLayer(msg);
      break;
    case "lookup_service":
      handleLookupService(msg);
      break;
    case "perform_service":
      handlePerformService(msg);
      break;
    case "equals": {
      const exec = executions.get(msg.exec);
      const a = exec?.handles.get(msg.a);
      const b = exec?.handles.get(msg.b);
      const equal = a !== undefined && b !== undefined && EEqual.equals(a.value, b.value);
      send({ type: "equals_done", equal });
      break;
    }
    case "cancel": {
      const exec = executions.get(msg.exec);
      const pending = exec?.pendingCalls.get(msg.call_id);
      if (pending) {
        exec.pendingCalls.delete(msg.call_id);
        pending.abort();
      }
      break;
    }
    case "cb_return": {
      const exec = executions.get(msg.exec);
      const pending = exec?.pendingCbs.get(msg.req_id);
      if (pending) {
        exec.pendingCbs.delete(msg.req_id);
        if (msg.ok) {
          let val = decodeWire(msg.value, exec, wireCtx);
          if (pending.retCopy) val = runtime.copyData(val);
          pending.resolve(val);
        } else {
          pending.reject(decodeFailure(msg, exec));
        }
      }
      break;
    }
    case "shutdown":
      ioWorker.postMessage(null);
      process.exit(0);
      break;
  }
}

workerState.dispatchSyncNested = dispatchSyncNested;
workerState.handleMessage = handleMessage;

ioWorker.on("message", (line) => {
  handleMessage(line).catch((err) => {
    send({
      type: "fatal",
      message: err instanceof Error ? err.stack ?? err.message : String(err),
    });
  });
});

send({ type: "ready" });
