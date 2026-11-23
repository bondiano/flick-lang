// Target resolution, Effect-TS / Flick value crossing, call context assembly,
// argument adaptation, and call completion encoding for the host bridge worker.
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { decodeWire, encodeWire } from "./wire.mjs";
import {
  EContext,
  Effect,
  bridge,
  bridgeOverrides,
  builtinModuleCache,
  causeMod,
  containsEff,
  effEffectsMod,
  effectSpecifier,
  fiberMod,
  fnResultHint,
  fnStages,
  hostFnUnwrap,
  isTrustedPackage,
  layerMod,
  outcomeMod,
  repoRoot,
  rootEffectHostServices,
  rootHostServices,
  routeMap,
  runtime,
  send,
  stubsMod,
  wireCtx,
} from "./worker_runtime.mjs";

export function hasAdapterWord(adapter, name, identArg = null) {
  const words = adapter?.words;
  if (!Array.isArray(words)) return false;
  return words.some(
    (w) =>
      w.name === name &&
      (identArg === null ||
        (Array.isArray(w.args) && w.args.length === 1 && w.args[0]?.Ident === identArg)),
  );
}

function isBridgedSpec(spec) {
  return (
    spec.mode === "Effect" ||
    spec.adapter.shapes.some(containsEff) ||
    containsEff(spec.adapter.result_shape)
  );
}

function resolveRelativePath(spec) {
  const dir = spec.source_dir ?? repoRoot;
  let resolvedPath = path.resolve(dir, spec.from);
  if (resolvedPath.endsWith(".mjs")) {
    const rtVariant = resolvedPath.slice(0, -4) + ".rt.mjs";
    if (fs.existsSync(rtVariant)) {
      resolvedPath = rtVariant;
    }
  }
  return resolvedPath;
}

export async function resolveTarget(exec, spec) {
  if (!spec.from) {
    let cur = globalThis;
    for (const part of spec.name.split(".")) {
      cur = cur?.[part];
    }
    return { target: cur, bridged: false };
  }
  const from = spec.from;
  if (from.startsWith("./") || from.startsWith("../")) {
    const resolvedPath = resolveRelativePath(spec);
    let mod = exec.moduleCache.get(resolvedPath);
    if (mod === undefined) {
      mod = await import(`${pathToFileURL(resolvedPath).href}?exec=${exec.id}`);
      exec.moduleCache.set(resolvedPath, mod);
    }
    return { target: mod[spec.name], bridged: isBridgedSpec(spec) };
  }

  // Pre-resolved `@flick-lang/runtime/*` or `@flick-lang/effect-ts/*` from compiler `resolve_routes`.
  if (from === "@flick-lang/runtime" || from.startsWith("@flick-lang/runtime/")) {
    let mod = builtinModuleCache.get(from);
    if (mod === undefined) {
      mod = await import(from);
      builtinModuleCache.set(from, mod);
    }
    return { target: mod[spec.name], bridged: false };
  }
  if (from === "@flick-lang/effect-ts" || from.startsWith("@flick-lang/effect-ts/")) {
    let mod = builtinModuleCache.get(from);
    if (mod === undefined) {
      mod = await import(from);
      builtinModuleCache.set(from, mod);
    }
    return { target: mod[spec.name], bridged: true };
  }

  // `flick:*` or third-party package specifier.
  const routed = routeMap.get(from);
  if (routed && !bridgeOverrides.has(`${routed} ${spec.name}`)) {
    let mod = builtinModuleCache.get(routed);
    if (mod === undefined) {
      mod = await import(routed);
      builtinModuleCache.set(routed, mod);
    }
    if (spec.name in mod && mod[spec.name] !== stubsMod[spec.name]) {
      const isOwnedRt =
        routed === "@flick-lang/runtime" || routed.startsWith("@flick-lang/runtime/");
      return { target: mod[spec.name], bridged: !isOwnedRt && isBridgedSpec(spec) };
    }
  }
  const effSpec = effectSpecifier(from);
  let mod = builtinModuleCache.get(effSpec);
  if (mod === undefined) {
    mod = await import(effSpec);
    builtinModuleCache.set(effSpec, mod);
  }
  return { target: mod[spec.name], bridged: true };
}

export function resolveTargetSync(exec, spec) {
  if (!spec.from) {
    let cur = globalThis;
    for (const part of spec.name.split(".")) cur = cur?.[part];
    return { target: cur, bridged: false, needsAsyncLoad: false };
  }
  const from = spec.from;
  if (from.startsWith("./") || from.startsWith("../")) {
    const mod = exec.moduleCache.get(resolveRelativePath(spec));
    if (!mod) return { needsAsyncLoad: true };
    return { target: mod[spec.name], bridged: isBridgedSpec(spec), needsAsyncLoad: false };
  }
  if (from === "@flick-lang/runtime" || from.startsWith("@flick-lang/runtime/")) {
    const mod = builtinModuleCache.get(from);
    if (!mod) return { needsAsyncLoad: true };
    return { target: mod[spec.name], bridged: false, needsAsyncLoad: false };
  }
  if (from === "@flick-lang/effect-ts" || from.startsWith("@flick-lang/effect-ts/")) {
    const mod = builtinModuleCache.get(from);
    if (!mod) return { needsAsyncLoad: true };
    return { target: mod[spec.name], bridged: true, needsAsyncLoad: false };
  }
  const routed = routeMap.get(from);
  if (routed && !bridgeOverrides.has(`${routed} ${spec.name}`)) {
    const mod = builtinModuleCache.get(routed);
    if (!mod) return { needsAsyncLoad: true };
    if (spec.name in mod && mod[spec.name] !== stubsMod[spec.name]) {
      const isOwnedRt =
        routed === "@flick-lang/runtime" || routed.startsWith("@flick-lang/runtime/");
      return {
        target: mod[spec.name],
        bridged: !isOwnedRt && isBridgedSpec(spec),
        needsAsyncLoad: false,
      };
    }
  }
  const effMod = builtinModuleCache.get(effectSpecifier(from));
  if (!effMod) return { needsAsyncLoad: true };
  return { target: effMod[spec.name], bridged: true, needsAsyncLoad: false };
}

export function crossValue(toward, shape, value) {
  if (!shape || shape === "Data") {
    return toward === "host" ? bridge.dataToEffect(value) : bridge.dataFromEffect(value);
  }
  if (typeof shape === "string") {
    const map = {
      Layer: toward === "host" ? bridge.layerToEffect : bridge.layerFromEffect,
      Schema: toward === "host" ? bridge.schemaToEffect : bridge.schemaFromEffect,
      Codec: toward === "host" ? bridge.codecToEffect : bridge.codecFromEffect,
      Fiber: toward === "host" ? bridge.fiberToEffect : bridge.fiberFromEffect,
      Duration: toward === "host" ? bridge.durationToEffect : bridge.durationFromEffect,
      Schedule: toward === "host" ? bridge.scheduleToEffect : bridge.scheduleFromEffect,
      Chunk: toward === "host" ? bridge.chunkToEffect : bridge.chunkFromEffect,
      HashMap: toward === "host" ? bridge.hashMapToEffect : bridge.hashMapFromEffect,
      HashSet: toward === "host" ? bridge.hashSetToEffect : bridge.hashSetFromEffect,
      Handlers: toward === "host" ? bridge.handlersToEffect : (v) => v,
      Exit: toward === "host" ? bridge.exitToEffect : bridge.exitFromEffect,
      Failure:
        toward === "host"
          ? (v) =>
              bridge.failureToEffect(v instanceof outcomeMod.Failure ? v : new outcomeMod.Failure(v))
          : bridge.failureFromEffect,
    };
    return map[shape] ? map[shape](value) : value;
  }
  if ("List" in shape) {
    return Array.isArray(value) ? value.map((x) => crossValue(toward, shape.List, x)) : value;
  }
  if ("Option" in shape) {
    return toward === "host"
      ? bridge.optionToEffect(value, (x) => crossValue(toward, shape.Option, x))
      : bridge.optionFromEffect(value, (x) => crossValue(toward, shape.Option, x));
  }
  if ("Result" in shape) {
    return toward === "host"
      ? bridge.resultToEffect(
          value,
          (a) => crossValue(toward, shape.Result[1], a),
          (e) => crossValue(toward, shape.Result[0], e),
        )
      : bridge.resultFromEffect(
          value,
          (a) => crossValue(toward, shape.Result[1], a),
          (e) => crossValue(toward, shape.Result[0], e),
        );
  }
  if ("Tuple" in shape && Array.isArray(value)) {
    return shape.Tuple.map((s, i) => crossValue(toward, s, value[i]));
  }
  if ("Record" in shape && value && typeof value === "object") {
    const out = { ...crossValue(toward, "Data", value) };
    for (const [k, s] of shape.Record) {
      if (s !== "Data") out[k] = crossValue(toward, s, value[k]);
    }
    return out;
  }
  if ("Eff" in shape) {
    if (toward === "host" && Effect.isEffect(value)) {
      return Effect.map(value, (r) => crossValue("host", shape.Eff, r));
    }
    if (toward === "host" && typeof value === "function") {
      return bridge.callEffect(() =>
        runtime.mapped(value(), (r) => crossValue("host", shape.Eff, r)),
      );
    }
    if (toward === "flick" && Effect.isEffect(value)) {
      return () => runtime.mapped(bridge.fromEffect(value), (r) => crossValue("flick", shape.Eff, r));
    }
  }
  if ("Fun" in shape && typeof value === "function") {
    if (toward === "host") {
      const unwrapped = hostFnUnwrap.get(value);
      if (unwrapped !== undefined && unwrapped.arity === shape.Fun.args.length) {
        return unwrapped.fn;
      }
    }
    const { args, result, runtime: rtFlag, sync } = shape.Fun;
    const back = toward === "host" ? "flick" : "host";
    const callCurried = (params) => {
      let cur = value;
      for (let i = 0; i < args.length; i++) {
        cur = cur(crossValue(back, args[i], params[i]));
      }
      if (toward === "host") {
        if (sync) return crossValue(toward, result, bridge.callSync(() => cur));
        if (rtFlag) {
          if (Effect.isEffect(cur)) {
            return Effect.map(cur, (r) => crossValue(toward, result, r));
          }
          return bridge.callEffect(() => runtime.mapped(cur, (r) => crossValue(toward, result, r)));
        }
        return crossValue(toward, result, cur);
      }
      if (sync) return crossValue(toward, result, bridge.inSyncContext(() => cur));
      if (rtFlag) return runtime.mapped(bridge.fromEffect(cur), (r) => crossValue(toward, result, r));
      return crossValue(toward, result, cur);
    };
    const build = (idx, acc) =>
      idx >= args.length ? callCurried(acc) : (p) => build(idx + 1, [...acc, p]);
    const wrapped = build(0, []);
    if (toward === "flick") {
      hostFnUnwrap.set(wrapped, { fn: value, arity: args.length });
    }
    return wrapped;
  }
  return value;
}

export function adaptResult(spec, bridged, raw) {
  const trusted = isTrustedPackage(spec.from);
  const copyRes = !trusted && spec.adapter.copies.result;
  let v = copyRes ? runtime.copyData(raw) : raw;
  if (spec.adapter.result.nullable) {
    v = runtime.fromNullable(v);
  }
  if (spec.adapter.result.unit) {
    v = undefined;
  }
  if (bridged) {
    v = crossValue("flick", spec.adapter.result_shape, v);
  }
  return v;
}

export function encodeCauseWire(cause, exec) {
  const reasons = cause.reasons.map((r) => {
    if (r._tag === "Fail") {
      return ["fail", encodeWire(r.error, "Any", false, exec, wireCtx)];
    }
    if (r._tag === "Die") {
      const msg = r.defect instanceof Error ? r.defect.message : String(r.defect);
      return ["die", msg];
    }
    return ["interrupt", r.fiberId ?? null];
  });
  return { reasons };
}

export function encodeThrown(err, exec, hasCatch) {
  if (err instanceof causeMod.Raise) {
    return { kind: "cause", cause: encodeCauseWire(err.cause, exec) };
  }
  if (hasCatch) {
    return { kind: "catch", error: encodeWire(err, "Opaque", false, exec, wireCtx) };
  }
  const msg = err instanceof Error ? err.message : String(err);
  return { kind: "die", message: msg };
}

export function buildCallCtx(exec, msg) {
  let ctx = fiberMod.currentCtx();
  for (const [tag, impl] of rootHostServices) {
    if (fiberMod.inCtx(ctx, () => fiberMod.lookup(tag)) === undefined) {
      ctx = fiberMod.extend(ctx, tag, impl);
    }
  }
  let effHost = EContext.isContext(ctx.host) ? ctx.host : EContext.empty();
  for (const [key, impl] of rootEffectHostServices) {
    if (!effHost.mapUnsafe.has(key)) {
      effHost = EContext.add(effHost, effEffectsMod.service(key), impl);
    }
  }
  if (Array.isArray(msg.services)) {
    for (const entry of msg.services) {
      if (entry[0] === "host_ctx") {
        const handleEntry = exec.handles.get(entry[1]);
        if (handleEntry !== undefined) {
          for (const [tag, impl] of layerMod.servicesOf(handleEntry.value)) {
            if (tag === layerMod.HOST) {
              effHost = EContext.isContext(impl) ? EContext.merge(effHost, impl) : effHost;
            } else {
              ctx = fiberMod.extend(ctx, tag, impl);
            }
          }
        }
      } else if (entry[0] === "service") {
        const key = entry[1];
        const impl = decodeWire(entry[2], exec, wireCtx);
        const effImpl = decodeWire(entry[2], exec, { ...wireCtx, bridgedService: true });
        ctx = fiberMod.extend(ctx, runtime.service(key), impl);
        effHost = EContext.add(effHost, effEffectsMod.service(key), effImpl);
      }
    }
  }
  ctx = fiberMod.hosted(ctx, effHost);
  if (msg.owner !== undefined && msg.owner !== null) {
    const owner = exec.getOwner(msg.owner);
    ctx = fiberMod.owned(ctx, owner);
    const evOwner =
      msg.evidence_owner !== undefined && msg.evidence_owner !== null
        ? exec.getOwner(msg.evidence_owner)
        : owner;
    ctx = fiberMod.evidenced(ctx, evOwner);
  }
  return ctx;
}

export function prepareArgs(exec, spec, rawWireArgs, bridged, controller) {
  const trusted = isTrustedPackage(spec.from);
  const copies = !trusted && spec.adapter.copies ? spec.adapter.copies : null;
  const decodeCtx = bridged ? { ...wireCtx, bridged: true } : wireCtx;
  // Decode every raw wire argument (and look up all foreign handles) before
  // running any copy or Effect-TS crossing that could re-enter the bridge.
  const decoded = [];
  for (let i = 0; i < spec.dicts; i++) {
    decoded.push(decodeWire(rawWireArgs[i], exec, decodeCtx));
  }
  const decodedValueArgs = [];
  for (let i = 0; i < spec.arity; i++) {
    const wire = rawWireArgs[spec.dicts + i];
    const cbArity = spec.adapter.callbacks[i];
    const cbCopies = copies?.callbacks?.[i];
    const argHint = spec.arg_hints?.[i] ?? "Any";
    const argShape = spec.adapter.shapes?.[i] ?? "Data";

    let val;
    let alreadyCrossed = false;
    const rootNode = wire.nodes[wire.root];
    if (rootNode[0] === "task") {
      const innerShape =
        typeof argShape === "object" && argShape !== null && "Eff" in argShape
          ? argShape.Eff
          : undefined;
      val = wireCtx.makeTask(exec, rootNode[1], innerShape, bridged);
      alreadyCrossed = true;
    } else if (
      rootNode[0] === "cb" &&
      ((cbArity !== null && cbArity !== undefined) ||
        (typeof argShape === "object" && argShape !== null && "Fun" in argShape))
    ) {
      const cbId = rootNode[1];
      const arity = cbArity ?? rootNode[2];
      const funShape =
        typeof argShape === "object" && argShape !== null && "Fun" in argShape
          ? argShape.Fun
          : null;
      const runtimeFlag = funShape ? funShape.runtime : rootNode[3];
      const syncFlag = funShape ? funShape.sync : rootNode[4];
      const funHint =
        typeof argHint === "object" && argHint !== null && "Fun" in argHint ? argHint.Fun : null;
      const cbMeta = {
        bridged,
        funShape,
        argCopies: cbCopies ? cbCopies[0] : [],
        retCopy: cbCopies ? cbCopies[1] : false,
        argHints: funHint ? funHint.args : [],
        retHint: funHint ? funHint.result : "Any",
      };
      val = wireCtx.makeCallback(exec, cbId, arity, runtimeFlag, syncFlag, cbMeta);
      alreadyCrossed = true;
    } else {
      val = decodeWire(wire, exec, decodeCtx);
    }
    decodedValueArgs.push({ val, alreadyCrossed, argShape });
  }
  const valueArgs = [];
  for (let i = 0; i < spec.arity; i++) {
    const copyArg = copies?.args?.[i] === true;
    let { val, alreadyCrossed, argShape } = decodedValueArgs[i];
    if (spec.adapter.optional.includes(i)) {
      if (bridged) {
        if (copyArg) val = runtime.copyData(val);
        if (!alreadyCrossed) val = crossValue("host", argShape, val);
        val = runtime.optionalArg(val);
      } else {
        val = runtime.optionalArg(val);
        if (copyArg) val = runtime.copyData(val);
      }
    } else {
      if (copyArg && !alreadyCrossed) {
        val = runtime.copyData(val);
      }
      if (bridged && !alreadyCrossed) {
        val = crossValue("host", argShape, val);
      }
    }
    valueArgs.push(val);
  }
  const finalArgs = [...decoded];
  if (hasAdapterWord(spec.adapter, "variadic") && valueArgs.length > 0) {
    const last = valueArgs.pop();
    finalArgs.push(...valueArgs);
    if (Array.isArray(last)) finalArgs.push(...last);
  } else {
    finalArgs.push(...valueArgs);
  }
  if (hasAdapterWord(spec.adapter, "cancel", "signal") && controller) {
    finalArgs.push(controller.signal);
  }
  return finalArgs;
}

export function invokeTarget(spec, target, finalArgs) {
  if (spec.arity === 0 && spec.taken === 0 && spec.dicts === 0 && spec.mode === "Pure") {
    return target;
  }
  if (hasAdapterWord(spec.adapter, "this") && finalArgs.length > spec.dicts) {
    const receiver = finalArgs.splice(spec.dicts, 1)[0];
    return target.apply(receiver, finalArgs);
  }
  return target(...finalArgs);
}

export function finishCallOk(exec, callId, spec, bridged, raw, rawWireArgs, finalArgs) {
  if (raw && typeof raw === "object") {
    if (
      rawWireArgs &&
      (spec.name === "atomState" || spec.name === "atomLocal" || spec.name === "flickState")
    ) {
      const firstArg = rawWireArgs[spec.dicts];
      const rootKind = firstArg?.nodes?.[firstArg.root]?.[0];
      if (rootKind === "i") exec.intAtoms.add(raw);
    } else if (
      finalArgs &&
      (spec.name === "keepAlive" ||
        spec.name === "flickReadonly" ||
        spec.name === "flickWithLabel" ||
        spec.name === "flickIdleTtl")
    ) {
      const src = finalArgs[finalArgs.length - 1];
      if (src && typeof src === "object" && exec.intAtoms.has(src)) {
        exec.intAtoms.add(raw);
      }
    } else if (
      (spec.name === "atomDerived" || spec.name === "readable") &&
      spec.arg_hints?.[0]?.Fun?.result === "Int"
    ) {
      exec.intAtoms.add(raw);
    }
  }
  const adapted = adaptResult(spec, bridged, raw);
  if (
    typeof adapted === "function" &&
    Array.isArray(spec.remaining_stages) &&
    spec.remaining_stages.length > 0
  ) {
    fnStages.set(adapted, spec.remaining_stages);
  }
  if (
    typeof adapted === "function" &&
    spec.result_hint &&
    typeof spec.result_hint === "object" &&
    "Fun" in spec.result_hint
  ) {
    fnResultHint.set(adapted, spec.result_hint.Fun);
  }
  const copyRes = !isTrustedPackage(spec.from) && spec.adapter.copies.result;
  const wire = encodeWire(adapted, spec.result_hint, copyRes, exec, wireCtx);
  const syncFd1 =
    typeof spec.from === "string" &&
    (spec.from.includes("ChildProcess") || spec.name === "processSpawn");
  if (syncFd1) {
    try {
      fs.writeSync(1, "\0");
    } catch {
      // Ignore if fd 1 is not writable.
    }
  }
  send({
    type: "call_done",
    call_id: callId,
    ok: true,
    value: wire,
    ...(syncFd1 ? { sync_fd1: true } : {}),
  });
}

export function finishCallErr(exec, callId, err, hasCatch) {
  const encoded = encodeThrown(err, exec, hasCatch);
  send({ type: "call_done", call_id: callId, ok: false, ...encoded });
}
