// Re-entrant synchronous and asynchronous Rust callback, task, and mutable-cell
// proxies for the persistent host/FFI bridge worker.
import { decodeWire, encodeWire } from "./wire.mjs";
import { crossValue } from "./worker_adapt.mjs";
import {
  EContext,
  Effect,
  bridge,
  causeMod,
  ctrl,
  fiberMod,
  ioWorker,
  layerMod,
  readSyncLine,
  runtime,
  send,
  wireCtx,
  workerState,
} from "./worker_runtime.mjs";

export function releaseWireHandles(exec, wires, extraHandle) {
  const ids = [];
  if (extraHandle !== undefined) ids.push(extraHandle);
  for (const w of wires ?? []) {
    for (const node of w?.nodes ?? []) {
      if (Array.isArray(node) && node[0] === "handle" && typeof node[1] === "number") {
        ids.push(node[1]);
      }
    }
  }
  if (ids.length > 0) exec.releaseHandles(ids);
}

export function decodeFailure(msg, exec) {
  if (msg.cause) {
    const cause = decodeWire(msg.cause, exec, wireCtx);
    return new causeMod.Raise(cause);
  }
  return new Error(msg.message ?? "callback failed");
}

export function snapshotCtxEntries(rtCtx, effCtx) {
  const rtEntries = [];
  const seen = new Set();
  for (let c = rtCtx; c !== null && c !== undefined; c = c.next) {
    if (c.tag !== null && c.tag !== undefined && !seen.has(c.tag)) {
      seen.add(c.tag);
      rtEntries.push([c.tag, c.impl]);
    }
  }
  rtEntries.reverse();
  const entries = rtEntries;
  let mergedHost = EContext.isContext(rtCtx?.host) ? rtCtx.host : undefined;
  if (EContext.isContext(effCtx)) {
    mergedHost = mergedHost ? EContext.merge(mergedHost, effCtx) : effCtx;
  }
  if (mergedHost) {
    entries.push([layerMod.HOST, mergedHost]);
  }
  return entries;
}

export function makeCancelEffect(exec, reqId, promise) {
  return Effect.callback((done) => {
    if (!exec.pendingCbs.has(reqId) || exec.closing) {
      done(Effect.void);
      return;
    }
    send({ type: "cb_cancel", exec: exec.id, req_id: reqId });
    promise.then(
      () => done(Effect.void),
      () => done(Effect.void),
    );
  });
}

function resumePendingEffect(exec, reqId, retCopy, mapValue) {
  const promise = new Promise((resolve, reject) => {
    exec.pendingCbs.set(reqId, { resolve, reject, retCopy });
  });
  return Effect.callback((resume) => {
    promise.then(
      (val) => {
        try {
          resume(Effect.succeed(mapValue ? mapValue(val) : val));
        } catch (err) {
          resume(Effect.die(err));
        }
      },
      (err) => {
        if (err instanceof causeMod.Raise) {
          resume(Effect.failCause(bridge.causeToEffect(err.cause)));
        } else {
          resume(Effect.die(err));
        }
      },
    );
    return makeCancelEffect(exec, reqId, promise);
  });
}

export function invokeSyncCallback(exec, cbId, rawArgs, cbMeta) {
  if (exec.closing) {
    throw new Error("execution closed");
  }
  const reqId = exec.nextCbReq++;
  const argsWire = rawArgs.map((arg, idx) => {
    const copy = cbMeta?.argCopies?.[idx] === true;
    const val = copy ? runtime.copyData(arg) : arg;
    const hint = cbMeta?.argHints?.[idx] ?? "Any";
    return encodeWire(val, hint, copy, exec, wireCtx);
  });
  const snap = snapshotCtxEntries(fiberMod.currentCtx(), undefined);
  const hostCtxHandle = snap.length > 0 ? exec.retainHandle(snap) : undefined;
  workerState.syncDepth += 1;
  const cur = Atomics.load(ctrl, 0);
  if (cur === 0 || cur === 4) {
    Atomics.compareExchange(ctrl, 0, cur, 1);
  }
  send({
    type: "cb_invoke",
    exec: exec.id,
    req_id: reqId,
    cb_id: cbId,
    mode: "sync",
    args: argsWire,
    ...(hostCtxHandle !== undefined ? { host_ctx_handle: hostCtxHandle } : {}),
  });
  try {
    for (;;) {
      const line = readSyncLine();
      if (line === null) {
        throw new Error("host bridge disconnected during sync callback");
      }
      const msg = JSON.parse(line);
      if (msg.op === "close_exec") {
        if (exec) {
          exec.closing = true;
          releaseWireHandles(exec, argsWire, hostCtxHandle);
        }
        queueMicrotask(() => {
          workerState.handleMessage(line).catch(() => {});
        });
        throw new Error("execution closed");
      }
      if (msg.op === "cb_return" && msg.req_id === reqId) {
        if (msg.closing === true) {
          exec.closing = true;
          releaseWireHandles(exec, argsWire, hostCtxHandle);
          throw new Error("execution closed");
        }
        if (msg.ok) {
          exec.lastSyncCbWasInt = msg.value?.nodes?.[msg.value.root]?.[0] === "i";
          const resDecodeCtx = cbMeta?.bridged ? { ...wireCtx, bridged: true } : wireCtx;
          let val = decodeWire(msg.value, exec, resDecodeCtx);
          if (cbMeta?.retCopy) val = runtime.copyData(val);
          return val;
        }
        if (msg.kind === "async_needed") {
          const eff = resumePendingEffect(
            exec,
            reqId,
            cbMeta?.retCopy === true,
            cbMeta?.bridged && cbMeta?.funShape?.result
              ? (val) => crossValue("host", cbMeta.funShape.result, val)
              : null,
          );
          return cbMeta?.bridgedService ? eff : bridge.fromEffect(eff);
        }
        const err = decodeFailure(msg, exec);
        if (cbMeta?.bridgedService) {
          return err instanceof causeMod.Raise
            ? Effect.failCause(bridge.causeToEffect(err.cause))
            : Effect.die(err);
        }
        throw err;
      }
      // Nested re-entrant command while waiting for our sync callback!
      workerState.dispatchSyncNested(msg);
    }
  } finally {
    workerState.syncDepth -= 1;
    if (workerState.syncDepth === 0) {
      Atomics.compareExchange(ctrl, 0, 4, 0);
      Atomics.compareExchange(ctrl, 0, 1, 0);
      ioWorker.postMessage({ wake: true });
    }
  }
}

export function invokeAsyncCallback(exec, cbId, rawArgs, cbMeta, mode, effCtx, rtCtx, reqIdOut) {
  if (exec.closing) {
    return Promise.reject(new Error("execution closed"));
  }
  const reqId = exec.nextCbReq++;
  if (reqIdOut) reqIdOut.id = reqId;
  const argsWire = rawArgs.map((arg, idx) => {
    const copy = cbMeta?.argCopies?.[idx] === true;
    const val = copy ? runtime.copyData(arg) : arg;
    const hint = cbMeta?.argHints?.[idx] ?? "Any";
    return encodeWire(val, hint, copy, exec, wireCtx);
  });
  const snap = snapshotCtxEntries(rtCtx ?? fiberMod.currentCtx(), effCtx);
  const hostCtxHandle = snap.length > 0 ? exec.retainHandle(snap) : undefined;
  return new Promise((resolve, reject) => {
    exec.pendingCbs.set(reqId, { resolve, reject, retCopy: cbMeta?.retCopy === true });
    send({
      type: "cb_invoke",
      exec: exec.id,
      req_id: reqId,
      cb_id: cbId,
      mode,
      args: argsWire,
      ...(hostCtxHandle !== undefined ? { host_ctx_handle: hostCtxHandle } : {}),
    });
  });
}

export function invokeSyncOp(exec, payload) {
  if (exec.closing) {
    throw new Error("execution closed");
  }
  const reqId = exec.nextCbReq++;
  workerState.syncDepth += 1;
  const cur = Atomics.load(ctrl, 0);
  if (cur === 0 || cur === 4) {
    Atomics.compareExchange(ctrl, 0, cur, 1);
  }
  send({ ...payload, exec: exec.id, req_id: reqId });
  try {
    for (;;) {
      const line = readSyncLine();
      if (line === null) {
        throw new Error("host bridge disconnected during sync operation");
      }
      const msg = JSON.parse(line);
      if (msg.op === "close_exec") {
        exec.closing = true;
        releaseWireHandles(exec, payload.args, payload.host_ctx_handle);
        if (payload.value) releaseWireHandles(exec, [payload.value], undefined);
        queueMicrotask(() => {
          workerState.handleMessage(line).catch(() => {});
        });
        throw new Error("execution closed");
      }
      if (msg.op === "cb_return" && msg.req_id === reqId) {
        if (msg.closing === true) {
          exec.closing = true;
          releaseWireHandles(exec, payload.args, payload.host_ctx_handle);
          if (payload.value) releaseWireHandles(exec, [payload.value], undefined);
          throw new Error("execution closed");
        }
        if (msg.ok) {
          exec.lastSyncCbWasInt = msg.value?.nodes?.[msg.value.root]?.[0] === "i";
        }
        return { ...msg, _reqId: reqId };
      }
      workerState.dispatchSyncNested(msg);
    }
  } finally {
    workerState.syncDepth -= 1;
    if (workerState.syncDepth === 0) {
      Atomics.compareExchange(ctrl, 0, 4, 0);
      Atomics.compareExchange(ctrl, 0, 1, 0);
      ioWorker.postMessage({ wake: true });
    }
  }
}

export function makeCellProxy(exec, cellId) {
  return {
    get value() {
      const msg = invokeSyncOp(exec, { type: "cell_get", cell_id: cellId });
      if (!msg.ok) throw decodeFailure(msg, exec);
      return decodeWire(msg.value, exec, wireCtx);
    },
    set value(v) {
      const wire = encodeWire(v, "Any", false, exec, wireCtx);
      const msg = invokeSyncOp(exec, { type: "cell_set", cell_id: cellId, value: wire });
      if (!msg.ok) throw decodeFailure(msg, exec);
    },
  };
}

export function makeTask(exec, cbId, innerShape, bridged) {
  const key = `task:${cbId}:${bridged ? 1 : 0}`;
  const existing = exec.callbacks.get(key);
  if (existing !== undefined && !innerShape) {
    return existing;
  }
  if (bridged) {
    return Effect.flatMap(Effect.context(), (effCtx) => {
      const snap = snapshotCtxEntries(fiberMod.currentCtx(), effCtx);
      const hostCtxHandle = snap.length > 0 ? exec.retainHandle(snap) : undefined;
      const msg = invokeSyncOp(exec, {
        type: "cb_invoke",
        cb_id: cbId,
        mode: "sync_task",
        args: [],
        ...(hostCtxHandle !== undefined ? { host_ctx_handle: hostCtxHandle } : {}),
      });
      if (msg.ok) {
        let val = decodeWire(msg.value, exec, wireCtx);
        if (innerShape) val = crossValue("host", innerShape, val);
        return Effect.succeed(val);
      }
      if (msg.kind === "async_needed") {
        return resumePendingEffect(
          exec,
          msg._reqId,
          false,
          innerShape ? (val) => crossValue("host", innerShape, val) : null,
        );
      }
      const err = decodeFailure(msg, exec);
      if (err instanceof causeMod.Raise) {
        return Effect.failCause(bridge.causeToEffect(err.cause));
      }
      return Effect.die(err);
    });
  }
  const fn = () =>
    (function* () {
      const snap = snapshotCtxEntries(fiberMod.currentCtx(), undefined);
      const hostCtxHandle = snap.length > 0 ? exec.retainHandle(snap) : undefined;
      const msg = invokeSyncOp(exec, {
        type: "cb_invoke",
        cb_id: cbId,
        mode: "sync_task",
        args: [],
        ...(hostCtxHandle !== undefined ? { host_ctx_handle: hostCtxHandle } : {}),
      });
      if (msg.ok) {
        return decodeWire(msg.value, exec, wireCtx);
      }
      if (msg.kind === "async_needed") {
        const eff = resumePendingEffect(exec, msg._reqId, false, null);
        return yield* bridge.fromEffect(eff);
      }
      throw decodeFailure(msg, exec);
    })();
  if (!innerShape) {
    exec.callbacks.set(key, fn);
  }
  return fn;
}

export function makeCallback(exec, cbId, arity, runtimeFlag, syncFlag, cbMeta) {
  const bridged = cbMeta?.bridged === true;
  const copyKey = cbMeta
    ? `${JSON.stringify(cbMeta.argCopies ?? [])}:${cbMeta.retCopy ? 1 : 0}`
    : "";
  const hasShape = Boolean(cbMeta?.funShape);
  const cacheKey = hasShape
    ? null
    : `${cbId}:${arity}:${runtimeFlag ? 1 : 0}:${syncFlag ? 1 : 0}:${bridged ? 1 : 0}:${cbMeta?.bridgedService ? 1 : 0}:${copyKey}`;
  if (cacheKey !== null) {
    const existing = exec.callbacks.get(cacheKey);
    if (existing !== undefined) {
      return existing;
    }
  }
  const fn = (...jsArgs) => {
    const passed = arity === 0 ? [undefined] : jsArgs.slice(0, arity);
    while (passed.length < arity) passed.push(undefined);
    const funShape = cbMeta?.funShape;
    const adaptedArgs =
      bridged && funShape
        ? passed.map((a, idx) => crossValue("flick", funShape.args[idx] ?? "Data", a))
        : passed;
    if (runtimeFlag && !syncFlag) {
      const argCopies = cbMeta?.argCopies ?? [];
      const argHints = cbMeta?.argHints ?? [];
      const runSyncOrAsync = (effCtx) => {
        const argsWire = adaptedArgs.map((a, idx) =>
          encodeWire(a, argHints[idx] ?? "Any", argCopies[idx] === true, exec, wireCtx),
        );
        const snap = snapshotCtxEntries(fiberMod.currentCtx(), effCtx);
        const hostCtxHandle = snap.length > 0 ? exec.retainHandle(snap) : undefined;
        const msg = invokeSyncOp(exec, {
          type: "cb_invoke",
          cb_id: cbId,
          mode: "sync_effect",
          args: argsWire,
          ...(hostCtxHandle !== undefined ? { host_ctx_handle: hostCtxHandle } : {}),
        });
        if (msg.ok) {
          let val = decodeWire(msg.value, exec, wireCtx);
          if (cbMeta?.retCopy) val = runtime.copyData(val);
          if (bridged && funShape?.result) val = crossValue("host", funShape.result, val);
          return { sync: true, value: val };
        }
        if (msg.kind === "async_needed") {
          const eff = resumePendingEffect(
            exec,
            msg._reqId,
            cbMeta?.retCopy === true,
            bridged && funShape?.result
              ? (val) => crossValue("host", funShape.result, val)
              : null,
          );
          return { sync: false, eff };
        }
        throw decodeFailure(msg, exec);
      };
      if (bridged) {
        return Effect.flatMap(Effect.context(), (effCtx) => {
          try {
            const res = runSyncOrAsync(effCtx);
            return res.sync ? Effect.succeed(res.value) : res.eff;
          } catch (err) {
            if (err instanceof causeMod.Raise) {
              return Effect.failCause(bridge.causeToEffect(err.cause));
            }
            return Effect.die(err);
          }
        });
      }
      return (function* () {
        const res = runSyncOrAsync(undefined);
        if (res.sync) return res.value;
        return yield* bridge.fromEffect(res.eff);
      })();
    }
    const raw = invokeSyncCallback(exec, cbId, adaptedArgs, cbMeta);
    return bridged && funShape ? crossValue("host", funShape.result, raw) : raw;
  };
  Object.defineProperty(fn, "length", { value: arity, configurable: true });
  if (cacheKey !== null) {
    exec.callbacks.set(cacheKey, fn);
  }
  return fn;
}
