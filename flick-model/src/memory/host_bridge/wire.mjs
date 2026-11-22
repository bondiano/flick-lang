// Stack-safe iterative flat-node wire decoder for the counted Rust <-> Node
// Host/FFI bridge, re-exporting `encodeWire` and tag registry helpers.

import {
  decodeCodecWitness,
  decodeRouteUrl,
  decodeSchemaWitness,
  fromUtf16Units,
  initWellKnownTags,
  registerTag,
  tupleArrays,
} from "./wire_witness.mjs";
import { encodeWire } from "./wire_encode.mjs";

export { encodeWire, initWellKnownTags, registerTag };

/**
 * Decodes a flat `{ root, nodes }` wire payload into a JS value iteratively.
 * Every node in `nodes` is constructed in post-order using an explicit stack.
 */
export function decodeWire(wire, exec, ctx) {
  const { runtime, makeCallback } = ctx;
  const nodes = wire.nodes;
  const n = nodes.length;
  const resolved = new Array(n);
  const state = new Uint8Array(n); // 0 = unvisited, 1 = children queued, 2 = done
  const stack = [wire.root];

  while (stack.length > 0) {
    const idx = stack[stack.length - 1];
    if (state[idx] === 2) {
      stack.pop();
      continue;
    }
    const node = nodes[idx];
    const kind = node[0];
    if (state[idx] === 0) {
      state[idx] = 1;
      if (kind === "list" || kind === "tuple") {
        const items = node[1];
        for (let i = items.length - 1; i >= 0; i--) {
          if (state[items[i]] === 0) stack.push(items[i]);
        }
        continue;
      }
      if (kind === "rec" || kind === "context") {
        const pairs = node[1];
        for (let i = pairs.length - 1; i >= 0; i--) {
          if (state[pairs[i][1]] === 0) stack.push(pairs[i][1]);
        }
        continue;
      }
      if (kind === "dict") {
        const entries = node[1];
        for (let i = entries.length - 1; i >= 0; i--) {
          const vid = entries[i].length === 3 ? entries[i][2] : entries[i][1];
          if (state[vid] === 0) stack.push(vid);
        }
        continue;
      }
      if (kind === "layer_merge_all") {
        const items = node[1];
        for (let i = items.length - 1; i >= 0; i--) {
          if (state[items[i]] === 0) stack.push(items[i]);
        }
        continue;
      }
      if (kind === "tag") {
        const fields = node[2];
        for (let i = fields.length - 1; i >= 0; i--) {
          if (state[fields[i]] === 0) stack.push(fields[i]);
        }
        continue;
      }
      if (kind === "layer_succeed" || kind === "layer_context" || kind === "exit_ok" || kind === "exit_fail") {
        const child = kind === "layer_succeed" ? node[2] : node[1];
        if (state[child] === 0) stack.push(child);
        continue;
      }
      if (kind === "layer_merge") {
        if (state[node[2]] === 0) stack.push(node[2]);
        if (state[node[1]] === 0) stack.push(node[1]);
        continue;
      }
      if (kind === "cause") {
        const reasons = node[1];
        for (let i = reasons.length - 1; i >= 0; i--) {
          const r = reasons[i];
          if ((r[0] === "fail" || r[0] === "die_val") && state[r[1]] === 0) {
            stack.push(r[1]);
          }
        }
        continue;
      }
    }

    stack.pop();
    state[idx] = 2;
    switch (kind) {
      case "u":
        resolved[idx] = undefined;
        break;
      case "null":
        resolved[idx] = null;
        break;
      case "b":
        resolved[idx] = Boolean(node[1]);
        break;
      case "i":
      case "f":
        resolved[idx] = Number(node[1]);
        break;
      case "fs":
        resolved[idx] =
          node[1] === "NaN"
            ? Number.NaN
            : node[1] === "Infinity"
              ? Number.POSITIVE_INFINITY
              : node[1] === "-Infinity"
                ? Number.NEGATIVE_INFINITY
                : -0;
        break;
      case "n":
        resolved[idx] = BigInt(node[1]);
        break;
      case "s":
        resolved[idx] = node[1];
        break;
      case "s16":
        resolved[idx] = fromUtf16Units(node[1]);
        break;
      case "bytes":
        resolved[idx] = new Uint8Array(Buffer.from(node[1], "base64"));
        break;
      case "dec":
        resolved[idx] = runtime.decimalFromString(node[1]);
        break;
      case "dur":
        if (node[1] === "ms") resolved[idx] = new ctx.Duration(node[2]);
        else if (node[1] === "ns") {
          const ns = BigInt(node[2]);
          resolved[idx] = new ctx.Duration(Number(ns) / 1_000_000, ns);
        } else if (node[1] === "inf") resolved[idx] = new ctx.Duration(Number.POSITIVE_INFINITY);
        else resolved[idx] = new ctx.Duration(0);
        break;
      case "dt": {
        if (ctx.EDateTime) {
          const z = node[2];
          if (Array.isArray(z) && z[0] === "offset") {
            resolved[idx] = ctx.EDateTime.makeZonedUnsafe(node[1], {
              timeZone: ctx.EDateTime.zoneMakeOffset(z[1]),
            });
          } else if (Array.isArray(z) && z[0] === "named") {
            resolved[idx] = ctx.EDateTime.makeZonedUnsafe(node[1], {
              timeZone: ctx.EDateTime.zoneMakeNamedUnsafe(z[1]),
            });
          } else {
            resolved[idx] = ctx.EDateTime.makeUnsafe(node[1]);
          }
        } else {
          resolved[idx] = { epochMilliseconds: node[1] };
        }
        break;
      }
      case "re":
        resolved[idx] = new RegExp(node[1], node[2]);
        break;
      case "list": {
        const ids = node[1];
        const arr = new Array(ids.length);
        for (let i = 0; i < ids.length; i++) arr[i] = resolved[ids[i]];
        resolved[idx] = arr;
        break;
      }
      case "tuple": {
        const ids = node[1];
        const arr = new Array(ids.length);
        for (let i = 0; i < ids.length; i++) arr[i] = resolved[ids[i]];
        tupleArrays.add(arr);
        resolved[idx] = arr;
        break;
      }
      case "rec": {
        const pairs = node[1];
        const obj = {};
        for (let i = 0; i < pairs.length; i++) {
          Object.defineProperty(obj, pairs[i][0], {
            value: resolved[pairs[i][1]],
            enumerable: true,
            writable: true,
            configurable: true,
          });
        }
        resolved[idx] = obj;
        break;
      }
      case "dict": {
        const entries = node[1];
        const obj = {};
        for (let i = 0; i < entries.length; i++) {
          const entry = entries[i];
          const k = entry[0];
          const isGetter = entry.length === 3 ? Boolean(entry[1]) : false;
          const vid = entry.length === 3 ? entry[2] : entry[1];
          if (isGetter) {
            const getter = resolved[vid];
            Object.defineProperty(obj, k, {
              get() {
                return getter();
              },
              enumerable: true,
              configurable: true,
            });
          } else {
            Object.defineProperty(obj, k, {
              value: resolved[vid],
              enumerable: true,
              writable: true,
              configurable: true,
            });
          }
        }
        resolved[idx] = obj;
        break;
      }
      case "tag": {
        const fullTag = node[1];
        const fields = node[2];
        if (fullTag === "Prelude.Option.Some") {
          resolved[idx] = runtime.some(resolved[fields[0]]);
        } else if (fullTag === "Prelude.Option.None") {
          resolved[idx] = runtime.none;
        } else if (fullTag === "Prelude.Result.Ok") {
          resolved[idx] = runtime.ok(resolved[fields[0]]);
        } else if (fullTag === "Prelude.Result.Err") {
          resolved[idx] = runtime.err(resolved[fields[0]]);
        } else if (fullTag === "Prelude.Ordering.LT") {
          resolved[idx] = runtime.LT;
        } else if (fullTag === "Prelude.Ordering.EQ") {
          resolved[idx] = runtime.EQ;
        } else if (fullTag === "Prelude.Ordering.GT") {
          resolved[idx] = runtime.GT;
        } else if (fullTag === "Redacted.Redacted.$Redacted") {
          resolved[idx] =
            ctx.bridged && ctx.Redacted
              ? ctx.Redacted.make(resolved[fields[0]])
              : ctx.runtime.redactedMake(resolved[fields[0]]);
        } else if (fullTag === "Route.Url.$Url" && ctx.routeMod) {
          resolved[idx] = decodeRouteUrl(fields.map((f) => resolved[f]), ctx);
        } else if (fullTag === "Codec.Codec.$Codec" && ctx.schemaMod) {
          resolved[idx] = decodeCodecWitness(fields.map((f) => resolved[f]), ctx);
        } else if (fullTag === "Schema.Schema.$Schema" && ctx.schemaMod) {
          resolved[idx] = decodeSchemaWitness(fields.map((f) => resolved[f]), ctx);
        } else if (fullTag === "Codec.$Standard.$Standard" && ctx.codecMod) {
          resolved[idx] = ctx.codecMod.codecStandard(resolved[fields[0]]);
        } else {
          const obj = { _tag: registerTag(fullTag, ctx.tagFn) };
          for (let i = 0; i < fields.length; i++) {
            obj[`_${i}`] = resolved[fields[i]];
          }
          resolved[idx] = obj;
        }
        break;
      }
      case "handle": {
        const entry = exec.handles.get(node[1]);
        if (entry === undefined) {
          throw new Error(`stale foreign handle ${node[1]}`);
        }
        resolved[idx] = entry.value;
        break;
      }
      case "cb": {
        const [, cbId, arity, runtimeFlag, syncFlag] = node;
        const meta =
          ctx.bridged || ctx.bridgedService
            ? {
                ...(ctx.bridged ? { bridged: true } : {}),
                ...(ctx.bridgedService ? { bridgedService: true } : {}),
              }
            : undefined;
        resolved[idx] = makeCallback(exec, cbId, arity, runtimeFlag, syncFlag, meta);
        break;
      }
      case "task": {
        const [, cbId] = node;
        resolved[idx] = ctx.makeTask(exec, cbId);
        break;
      }
      case "cell": {
        const [, cellId] = node;
        resolved[idx] = ctx.makeCellProxy(exec, cellId);
        break;
      }
      case "owner":
        resolved[idx] = exec.getOwner(node[1]);
        break;
      case "stag":
        resolved[idx] = runtime.service(node[1]);
        break;
      case "context": {
        const pairs = node[1];
        const entries = [];
        for (let i = 0; i < pairs.length; i++) {
          const k = pairs[i][0];
          const v = resolved[pairs[i][1]];
          if (k === "flick/bridge/HostContext" && Array.isArray(v)) {
            for (const sub of v) {
              for (const pair of ctx.layerMod.servicesOf(sub)) {
                entries.push(pair);
              }
            }
          } else {
            entries.push([runtime.service(k), v]);
          }
        }
        resolved[idx] = entries;
        break;
      }
      case "layer_empty":
        resolved[idx] = ctx.layerEmpty;
        break;
      case "layer_context":
        resolved[idx] = ctx.layerMod.constant(resolved[node[1]]);
        break;
      case "layer_succeed":
        resolved[idx] = runtime.layerSucceed(runtime.service(node[1]), resolved[node[2]]);
        break;
      case "layer_merge":
        resolved[idx] = runtime.layerMerge(resolved[node[1]], resolved[node[2]]);
        break;
      case "layer_merge_all": {
        const ids = node[1];
        const ls = ids.map((id) => resolved[id]);
        resolved[idx] = ctx.layerMod.mergeAll(ls);
        break;
      }
      case "exit_ok":
        resolved[idx] = ctx.cause.succeeded(resolved[node[1]]);
        break;
      case "exit_fail":
        resolved[idx] = ctx.cause.failed(resolved[node[1]]);
        break;
      case "cause": {
        const reasons = node[1].map((r) => {
          if (r[0] === "fail") return ctx.cause.failReason(resolved[r[1]]);
          if (r[0] === "die") {
            const d = r[1];
            if (typeof d === "string" && d.startsWith("Symbol(") && d.endsWith(")")) {
              return ctx.cause.dieReason(Symbol.for(d.slice(7, -1)));
            }
            return ctx.cause.dieReason(new Error(d));
          }
          if (r[0] === "die_val") return ctx.cause.dieReason(resolved[r[1]]);
          return ctx.cause.interruptReason(r[1] ?? 0);
        });
        resolved[idx] = ctx.cause.fromReasons(reasons);
        break;
      }
      default:
        throw new Error(`unknown wire node kind: ${kind}`);
    }
  }
  return resolved[wire.root];
}
