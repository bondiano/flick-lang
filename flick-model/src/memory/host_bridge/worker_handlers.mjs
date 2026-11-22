// Command handlers for foreign calls, host-handle invocation/awaiting, layer
// building, service lookup/perform, property reads, and owner closing.
import { decodeWire, encodeWire } from "./wire.mjs";
import {
  buildCallCtx,
  finishCallErr,
  finishCallOk,
  invokeTarget,
  prepareArgs,
  resolveTarget,
  resolveTargetSync,
} from "./worker_adapt.mjs";
import { invokeAsyncCallback, makeCallback } from "./worker_callbacks.mjs";
import {
  EContext,
  Effect,
  bridge,
  causeMod,
  collectMsgHandleIds,
  executions,
  fiberMod,
  fnResultHint,
  fnStages,
  layerMod,
  runtime,
  send,
  syncMod,
  wireCtx,
} from "./worker_runtime.mjs";

export async function handleCall(msg, alreadyPending = false, prePinned = null) {
  const exec = executions.get(msg.exec);
  const spec = msg.spec;
  const callId = msg.call_id;
  let pinned = prePinned ?? (exec ? exec.pinExistingHandles(collectMsgHandleIds(msg)) : []);
  const releasePinned = () => {
    if (pinned.length > 0) {
      const ids = pinned;
      pinned = [];
      exec?.releaseHandles(ids);
    }
  };
  const checkAlive = () => !alreadyPending || (exec.pendingCalls.delete(callId) && !exec.closing);
  try {
    // Special-case `Js.promise_callback` regardless of mode so the returned JS function
    // invokes the Rust async callback on a child fiber and resolves its Promise.
    if (
      (spec.from === "flick:rt" ||
        spec.from === "@flick-lang/runtime/rt" ||
        spec.from === "@flick-lang/effect-ts") &&
      spec.name === "promiseCallback"
    ) {
      const cbWire = msg.args[0];
      const cbId = cbWire.nodes[cbWire.root][1];
      const callCtx = buildCallCtx(exec, msg);
      const owner = callCtx.owner;
      const promiseFn = (arg) => {
        if (owner && !owner.open) {
          return Promise.reject(new runtime.RuntimeFailure(causeMod.interrupt(undefined)));
        }
        const p = invokeAsyncCallback(
          exec,
          cbId,
          [arg],
          undefined,
          "promise",
          undefined,
          callCtx,
        ).catch((err) => {
          if (err instanceof causeMod.Raise) {
            throw new runtime.RuntimeFailure(err.cause);
          }
          if (err instanceof runtime.RuntimeFailure) {
            throw err;
          }
          throw new runtime.RuntimeFailure(causeMod.die(err));
        });
        owner?.track(p);
        return p;
      };
      releasePinned();
      if (checkAlive()) finishCallOk(exec, callId, spec, false, promiseFn);
      return;
    }
    const syncResolved = resolveTargetSync(exec, spec);
    const { target, bridged } = syncResolved.needsAsyncLoad
      ? await resolveTarget(exec, spec)
      : syncResolved;
    if (alreadyPending && (!exec.pendingCalls.has(callId) || exec.closing)) return;
    const controller = spec.mode === "Async" || spec.adapter.cancel ? new AbortController() : null;
    const finalArgs = prepareArgs(exec, spec, msg.args, bridged, controller);
    const callCtx = buildCallCtx(exec, msg);
    releasePinned();

    if (spec.mode === "Pure" || !spec.runtime) {
      const raw = fiberMod.inCtx(callCtx, () =>
        bridged
          ? bridge.inSyncContext(() => invokeTarget(spec, target, finalArgs))
          : invokeTarget(spec, target, finalArgs),
      );
      if (checkAlive()) finishCallOk(exec, callId, spec, bridged, raw, msg.args, finalArgs);
      return;
    }

    if (spec.mode === "Sync") {
      const raw = fiberMod.inCtx(callCtx, () =>
        bridged
          ? bridge.inSyncContext(() => invokeTarget(spec, target, finalArgs))
          : invokeTarget(spec, target, finalArgs),
      );
      if (checkAlive()) finishCallOk(exec, callId, spec, bridged, raw, msg.args, finalArgs);
      return;
    }

    if (spec.mode === "Async") {
      const raw = fiberMod.inCtx(callCtx, () => invokeTarget(spec, target, finalArgs));
      const promise = Promise.resolve(raw);
      exec.pendingCalls.set(callId, { abort: () => controller?.abort() });
      callCtx.owner?.track(promise);
      if (!alreadyPending) send({ type: "pending", call_id: callId });
      promise.then(
        (val) => {
          if (!exec.pendingCalls.delete(callId) || exec.closing) return;
          try {
            finishCallOk(exec, callId, spec, bridged, val);
          } catch (err) {
            finishCallErr(exec, callId, err, spec.has_catch);
          }
        },
        (err) => {
          if (!exec.pendingCalls.delete(callId) || exec.closing) return;
          finishCallErr(exec, callId, err, spec.has_catch);
        },
      );
      return;
    }

    if (spec.mode === "Effect") {
      let made;
      try {
        made = fiberMod.inCtx(callCtx, () => invokeTarget(spec, target, finalArgs));
      } catch (err) {
        if (checkAlive()) finishCallErr(exec, callId, err, spec.has_catch);
        return;
      }
      const runOrVal = Effect.isEffect(made)
        ? fiberMod.inCtx(callCtx, () => bridge.fromEffect(made))
        : made;
      if (fiberMod.isRun(runOrVal) || runOrVal instanceof fiberMod.Instruction) {
        const fiber = fiberMod.forkNow(() => runOrVal, callCtx);
        if (fiber.exit !== undefined) {
          if (!checkAlive()) return;
          if (fiber.exit.cause === undefined) {
            finishCallOk(exec, callId, spec, bridged, fiber.exit.value);
          } else {
            finishCallErr(exec, callId, new causeMod.Raise(fiber.exit.cause), false);
          }
          return;
        }
        exec.pendingCalls.set(callId, { abort: () => fiber.interrupt(0) });
        if (!alreadyPending) send({ type: "pending", call_id: callId });
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
      if (checkAlive()) finishCallOk(exec, callId, spec, bridged, runOrVal);
    }
  } catch (err) {
    if (checkAlive()) finishCallErr(exec, callId, err, spec.has_catch);
  } finally {
    releasePinned();
  }
}

export function handleCallHandle(msg) {
  const exec = executions.get(msg.exec);
  const callId = msg.call_id;
  try {
    const entry = exec.handles.get(msg.handle);
    if (!entry || typeof entry.value !== "function") {
      throw new TypeError(`handle ${msg.handle} is not callable`);
    }
    const arg = decodeWire(msg.arg, exec, wireCtx);
    const callCtx = buildCallCtx(exec, msg);
    const stages = fnStages.get(entry.value);
    const funHint = fnResultHint.get(entry.value);
    const isRuntime = stages ? Boolean(stages[0]) : true;
    const out = fiberMod.inCtx(callCtx, () => entry.value(arg));
    let resultHint = entry.value._flickResultHint ?? "Any";
    if (funHint) {
      if (funHint.args.length > 1) {
        if (typeof out === "function") {
          fnResultHint.set(out, { args: funHint.args.slice(1), result: funHint.result });
        }
      } else {
        resultHint = funHint.result;
      }
    }
    if (typeof out === "function" && stages && stages.length > 1) {
      fnStages.set(out, stages.slice(1));
    }
    if (!isRuntime) {
      const wire = encodeWire(out, resultHint, false, exec, wireCtx);
      send({ type: "call_done", call_id: callId, ok: true, value: wire });
      return;
    }
    const runOrVal = Effect.isEffect(out)
      ? fiberMod.inCtx(callCtx, () => bridge.fromEffect(out))
      : out;
    if (fiberMod.isRun(runOrVal) || runOrVal instanceof fiberMod.Instruction) {
      const fiber = fiberMod.forkNow(() => runOrVal, callCtx);
      if (fiber.exit !== undefined) {
        if (fiber.exit.cause === undefined) {
          const wire = encodeWire(fiber.exit.value, resultHint, false, exec, wireCtx);
          send({ type: "call_done", call_id: callId, ok: true, value: wire });
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
          const wire = encodeWire(exit.value, resultHint, false, exec, wireCtx);
          send({ type: "call_done", call_id: callId, ok: true, value: wire });
        } else {
          finishCallErr(exec, callId, new causeMod.Raise(exit.cause), false);
        }
      });
      return;
    }
    const wire = encodeWire(runOrVal, resultHint, false, exec, wireCtx);
    send({ type: "call_done", call_id: callId, ok: true, value: wire });
  } catch (err) {
    finishCallErr(exec, callId, err, false);
  }
}

export function handleAwaitHandle(msg) {
  const exec = executions.get(msg.exec);
  const callId = msg.call_id;
  try {
    const entry = exec.handles.get(msg.handle);
    if (!entry) throw new TypeError(`stale handle ${msg.handle}`);
    const callCtx = buildCallCtx(exec, msg);
    const val = entry.value;
    const runOrVal = Effect.isEffect(val)
      ? fiberMod.inCtx(callCtx, () => bridge.fromEffect(val))
      : typeof val === "function"
        ? fiberMod.inCtx(callCtx, () => val())
        : val;
    if (fiberMod.isRun(runOrVal) || runOrVal instanceof fiberMod.Instruction) {
      const fiber = fiberMod.forkNow(() => runOrVal, callCtx);
      if (fiber.exit !== undefined) {
        if (fiber.exit.cause === undefined) {
          const hint = Effect.isEffect(fiber.exit.value) ? "Opaque" : "Any";
          const wire = encodeWire(fiber.exit.value, hint, false, exec, wireCtx);
          send({ type: "call_done", call_id: callId, ok: true, value: wire });
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
          const hint = Effect.isEffect(exit.value) ? "Opaque" : "Any";
          const wire = encodeWire(exit.value, hint, false, exec, wireCtx);
          send({ type: "call_done", call_id: callId, ok: true, value: wire });
        } else {
          finishCallErr(exec, callId, new causeMod.Raise(exit.cause), false);
        }
      });
      return;
    }
    const hint = Effect.isEffect(runOrVal) ? "Opaque" : "Any";
    const wire = encodeWire(runOrVal, hint, false, exec, wireCtx);
    send({ type: "call_done", call_id: callId, ok: true, value: wire });
  } catch (err) {
    finishCallErr(exec, callId, err, false);
  }
}

export function handleBuildLayer(msg) {
  const exec = executions.get(msg.exec);
  const callId = msg.call_id;
  try {
    const entry = exec.handles.get(msg.handle);
    if (!entry) throw new TypeError(`stale layer handle ${msg.handle}`);
    const callCtx = buildCallCtx(exec, msg);
    const owner = exec.getOwner(msg.owner);
    const adopted = layerMod.layerHooks.adopt(entry.value);
    const memo = fiberMod.inCtx(callCtx, () => layerMod.currentMemo());
    const run = fiberMod.inCtx(callCtx, () => layerMod.buildWith(adopted, memo, owner));
    const fiber = fiberMod.forkNow(() => run, callCtx);
    if (fiber.exit !== undefined) {
      if (fiber.exit.cause === undefined) {
        const hid = exec.retainHandle(fiber.exit.value);
        send({ type: "call_done", call_id: callId, ok: true, value: { root: 0, nodes: [["handle", hid]] } });
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
        const hid = exec.retainHandle(exit.value);
        send({ type: "call_done", call_id: callId, ok: true, value: { root: 0, nodes: [["handle", hid]] } });
      } else {
        finishCallErr(exec, callId, new causeMod.Raise(exit.cause), false);
      }
    });
  } catch (err) {
    finishCallErr(exec, callId, err, false);
  }
}

export function lookupInCallCtx(callCtx, key) {
  const found = fiberMod.inCtx(callCtx, () => fiberMod.lookup(runtime.service(key)));
  if (found !== undefined) return { impl: found, isEffect: false };
  try {
    const syncCtx = syncMod.runIn(Effect.context());
    const syncVal = syncCtx?.mapUnsafe?.get(key);
    if (syncVal !== undefined) return { impl: syncVal, isEffect: true };
  } catch {
    // Not in an Effect-TS sync context.
  }
  if (EContext.isContext(callCtx.host)) {
    const val = callCtx.host.mapUnsafe?.get(key);
    if (val !== undefined) return { impl: val, isEffect: true };
  }
  return null;
}

export function handleLookupService(msg) {
  const exec = executions.get(msg.exec);
  const callId = msg.call_id;
  try {
    const callCtx = buildCallCtx(exec, msg);
    const found = lookupInCallCtx(callCtx, msg.key);
    if (!found) {
      send({ type: "call_done", call_id: callId, ok: false, kind: "missing_service" });
      return;
    }
    const hid = exec.retainHandle(found.impl);
    send({ type: "call_done", call_id: callId, ok: true, value: { root: 0, nodes: [["handle", hid]] } });
  } catch (err) {
    finishCallErr(exec, callId, err, false);
  }
}

export function handlePerformService(msg) {
  const exec = executions.get(msg.exec);
  const callId = msg.call_id;
  let pinned = exec ? exec.pinExistingHandles(collectMsgHandleIds(msg)) : [];
  const releasePinned = () => {
    if (pinned.length > 0) {
      const ids = pinned;
      pinned = [];
      exec?.releaseHandles(ids);
    }
  };
  try {
    const callCtx = buildCallCtx(exec, msg);
    let impl;
    let isEffect = false;
    if (msg.handle !== undefined && msg.handle !== null) {
      const entry = exec.handles.get(msg.handle);
      if (!entry) throw new TypeError(`stale service handle ${msg.handle}`);
      impl = entry.value;
    } else {
      const found = lookupInCallCtx(callCtx, msg.effect);
      if (!found) {
        releasePinned();
        send({
          type: "call_done",
          call_id: callId,
          ok: false,
          kind: "missing_handler",
          effect: msg.effect,
          op_name: msg.op_name,
        });
        return;
      }
      impl = found.impl;
      isEffect = found.isEffect;
    }
    const fn = impl?.[msg.op_name];
    if (typeof fn !== "function") {
      releasePinned();
      send({
        type: "call_done",
        call_id: callId,
        ok: false,
        kind: "missing_handler",
        effect: msg.effect ?? "HostService",
        op_name: msg.op_name,
      });
      return;
    }
    const decodedArgs = msg.args.map((w) => {
      const v = decodeWire(w, exec, wireCtx);
      return isEffect ? bridge.dataToEffect(v) : v;
    });
    releasePinned();
    if (msg.op_name === "update" || msg.op_name === "modify") {
      const pair =
        decodedArgs.length >= 2 &&
        decodedArgs[0] &&
        typeof decodedArgs[0] === "object" &&
        exec.intAtoms.has(decodedArgs[0])
          ? 1
          : decodedArgs.length >= 3 &&
              decodedArgs[1] &&
              typeof decodedArgs[1] === "object" &&
              exec.intAtoms.has(decodedArgs[1])
            ? 2
            : -1;
      if (pair >= 0) {
        const cbWire = msg.args[pair];
        const cbNode = cbWire?.nodes?.[cbWire.root];
        if (cbNode && cbNode[0] === "cb") {
          decodedArgs[pair] = makeCallback(exec, cbNode[1], cbNode[2], cbNode[3], cbNode[4], {
            argHints: ["Int"],
          });
        }
      }
    }
    const isAtomRead =
      (msg.op_name === "get" || msg.op_name === "current") &&
      decodedArgs.length >= 1 &&
      decodedArgs[0] &&
      typeof decodedArgs[0] === "object";
    let resultHint = msg.result_hint ?? "Any";
    if (resultHint === "Any" && isAtomRead && exec.intAtoms.has(decodedArgs[0])) {
      resultHint = "Int";
    }
    const invokeCurried = () => {
      let cur = fn;
      if (decodedArgs.length === 0) {
        cur = cur(undefined);
      } else {
        for (const a of decodedArgs) {
          cur = cur(a);
        }
      }
      return cur;
    };
    if (isAtomRead) exec.lastSyncCbWasInt = false;
    const made = fiberMod.inCtx(callCtx, invokeCurried);
    if (isAtomRead && resultHint === "Any" && exec.lastSyncCbWasInt) {
      exec.intAtoms.add(decodedArgs[0]);
      resultHint = "Int";
    }
    const runOrVal = Effect.isEffect(made)
      ? fiberMod.inCtx(callCtx, () => bridge.fromEffect(made))
      : made;
    if (fiberMod.isRun(runOrVal) || runOrVal instanceof fiberMod.Instruction) {
      const fiber = fiberMod.forkNow(() => runOrVal, callCtx);
      if (fiber.exit !== undefined) {
        if (fiber.exit.cause === undefined) {
          const val = isEffect ? bridge.dataFromEffect(fiber.exit.value) : fiber.exit.value;
          const wire = encodeWire(val, resultHint, false, exec, wireCtx);
          send({ type: "call_done", call_id: callId, ok: true, value: wire });
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
          const val = isEffect ? bridge.dataFromEffect(exit.value) : exit.value;
          const wire = encodeWire(val, resultHint, false, exec, wireCtx);
          send({ type: "call_done", call_id: callId, ok: true, value: wire });
        } else {
          finishCallErr(exec, callId, new causeMod.Raise(exit.cause), false);
        }
      });
      return;
    }
    const val = isEffect ? bridge.dataFromEffect(runOrVal) : runOrVal;
    const wire = encodeWire(val, resultHint, false, exec, wireCtx);
    send({ type: "call_done", call_id: callId, ok: true, value: wire });
  } catch (err) {
    finishCallErr(exec, callId, err, false);
  } finally {
    releasePinned();
  }
}

export function handleGetField(msg) {
  const exec = executions.get(msg.exec);
  const callId = msg.call_id;
  try {
    const entry = exec.handles.get(msg.handle);
    if (!entry) throw new TypeError(`stale handle ${msg.handle}`);
    const val = Reflect.get(Object(entry.value), msg.field);
    const wire = encodeWire(val, "Any", false, exec, wireCtx);
    send({ type: "call_done", call_id: callId, ok: true, value: wire });
  } catch (err) {
    finishCallErr(exec, callId, err, false);
  }
}
