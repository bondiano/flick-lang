// Stack-safe iterative flat-node wire encoder for the counted Host/FFI bridge.
// Never touches getters or proxy traps on `"Opaque"` hints (`JsValue`, opaque handles).

import {
  hasLoneSurrogate,
  opaqueObjects,
  tagHashToFull,
  toUtf16Units,
  tupleArrays,
} from "./wire_witness.mjs";

function hintKind(hint) {
  if (typeof hint === "string") return hint;
  if (hint && typeof hint === "object") {
    const keys = Object.keys(hint);
    if (keys.length === 1) return keys[0];
  }
  return "Any";
}

function ownStringProp(obj, key) {
  const desc = Object.getOwnPropertyDescriptor(obj, key);
  if (desc && "value" in desc && typeof desc.value === "string") {
    return desc.value;
  }
  return undefined;
}

/**
 * Encodes a JS value into a flat `{ root, nodes }` wire payload iteratively.
 * Never touches getters or proxy traps on `"Opaque"` hints (`JsValue`, opaque handles).
 */
export function encodeWire(rootVal, rootHint, copy, exec, ctx) {
  const nodes = [];
  const active = new WeakSet();
  const work = [];
  let rootIdx = 0;

  function emit(node) {
    const id = nodes.length;
    nodes.push(node);
    return id;
  }

  function visit(val, hint, put) {
    const hk = hintKind(hint);
    if (hk === "Unit") {
      put(emit(["u"]));
      return;
    }
    if (val === undefined) {
      put(emit(["u"]));
      return;
    }
    if (val === null) {
      put(emit(["null"]));
      return;
    }
    const t = typeof val;
    if (t === "boolean") {
      put(emit(["b", val]));
      return;
    }
    if (t === "number") {
      if (Number.isNaN(val)) put(emit(["fs", "NaN"]));
      else if (val === Number.POSITIVE_INFINITY) put(emit(["fs", "Infinity"]));
      else if (val === Number.NEGATIVE_INFINITY) put(emit(["fs", "-Infinity"]));
      else if (Object.is(val, -0)) put(emit(["fs", "-0"]));
      else if (hk === "Int") put(emit(["i", val | 0]));
      else put(emit(["f", val]));
      return;
    }
    if (t === "bigint") {
      put(emit(["n", val.toString()]));
      return;
    }
    if (t === "string") {
      if (hasLoneSurrogate(val)) put(emit(["s16", toUtf16Units(val)]));
      else put(emit(["s", val]));
      return;
    }
    if (t === "function" || t === "symbol") {
      put(emit(["handle", exec.retainHandle(val)]));
      return;
    }

    // Object value: NEVER touch properties on Opaque hints (JsValue, getters, Proxy traps).
    if (hk === "Opaque" || (hk === "Any" && opaqueObjects.has(val))) {
      if (hk === "Opaque") opaqueObjects.add(val);
      put(emit(["handle", exec.retainHandle(val)]));
      return;
    }

    // Mirrored native Owner identity
    const ownerId = exec.ownerIdOf(val);
    if (ownerId !== undefined) {
      put(emit(["owner", ownerId]));
      return;
    }

    if (val instanceof Uint8Array) {
      put(emit(["bytes", Buffer.from(val.buffer, val.byteOffset, val.byteLength).toString("base64")]));
      return;
    }
    if (val instanceof RegExp) {
      put(emit(["re", val.source, val.flags]));
      return;
    }
    if (ctx.Duration && val instanceof ctx.Duration) {
      if (!Number.isFinite(val.millis)) put(emit(["dur", "inf"]));
      else if (val.nanos !== undefined) put(emit(["dur", "ns", val.nanos.toString()]));
      else put(emit(["dur", "ms", val.millis]));
      return;
    }
    if (ctx.EDuration && ctx.EDuration.isDuration(val)) {
      const nativeDur = ctx.bridge.durationFromEffect(val);
      if (!Number.isFinite(nativeDur.millis)) put(emit(["dur", "inf"]));
      else if (nativeDur.nanos !== undefined) put(emit(["dur", "ns", nativeDur.nanos.toString()]));
      else put(emit(["dur", "ms", nativeDur.millis]));
      return;
    }
    if (ctx.EDateTime && ctx.EDateTime.isDateTime(val)) {
      const ms = ctx.EDateTime.toEpochMillis(val);
      if (ctx.EDateTime.isZoned(val) && val.zone) {
        if (val.zone._tag === "Offset") {
          put(emit(["dt", ms, ["offset", val.zone.offset]]));
          return;
        }
        if (val.zone._tag === "Named") {
          put(emit(["dt", ms, ["named", val.zone.id]]));
          return;
        }
      }
      put(emit(["dt", ms]));
      return;
    }
    if (ctx.Redacted && ctx.Redacted.isRedacted(val)) {
      const inner = ctx.Redacted.value(val);
      work.push(() => {
        visit(inner, "Any", (cid) => {
          put(emit(["tag", "Redacted.Redacted.$Redacted", [cid]]));
        });
      });
      return;
    }
    if (
      typeof val.toJSON === "function" &&
      typeof val.toString === "function" &&
      val.toJSON === Object.getOwnPropertyDescriptor(val, "toJSON")?.value &&
      val.toString() === "<redacted>"
    ) {
      try {
        const inner = ctx.runtime.redactedReveal(val);
        work.push(() => {
          visit(inner, "Any", (cid) => {
            put(emit(["tag", "Redacted.Redacted.$Redacted", [cid]]));
          });
        });
        return;
      } catch {
        // Not a native Redacted wrapper.
      }
    }
    const isEffectSchema = ctx.ESchema && ctx.ESchema.isSchema(val);
    const isNativeWitness =
      typeof val.decode === "function" &&
      typeof val.encode === "function" &&
      typeof val.decodeAt === "function" &&
      typeof val.encodeAt === "function";
    if (isEffectSchema || isNativeWitness) {
      const native = isEffectSchema ? ctx.bridge.codecFromEffect(val) : val;
      const decId = emit(["handle", exec.retainHandle((input) => native.decode(input))]);
      const encId = emit(["handle", exec.retainHandle((input) => native.encode(input))]);
      const accId = emit(["handle", exec.retainHandle((input) => native.decode(input)._tag === "Success")]);
      let reason = "a host codec has no JSON Schema form";
      const hostExp = ctx.runtime?.codecJsonSchema?.("__", native);
      if (hostExp && hostExp._tag === "Failure" && typeof hostExp.failure?._0 === "string") {
        const prefix = "__ -> JSON Schema: ";
        reason = hostExp.failure._0.startsWith(prefix)
          ? hostExp.failure._0.slice(prefix.length)
          : hostExp.failure._0;
      }
      const reasonId = emit(["s", reason]);
      const metaId = emit(["tag", "Codec.$Meta.$Refused", [reasonId]]);
      const extraId = emit(["tag", "Codec.$Extra.$None", []]);
      put(emit(["tag", "Codec.Codec.$Codec", [decId, encId, accId, metaId, extraId]]));
      return;
    }
    if (ctx.EOption && ctx.EOption.isOption(val)) {
      if (ctx.EOption.isNone(val)) {
        put(emit(["tag", "Prelude.Option.None", []]));
      } else {
        const innerHint = hk === "Option" ? hint.Option : "Any";
        const childVal = val.value;
        work.push(() => {
          visit(childVal, innerHint, (cid) => {
            put(emit(["tag", "Prelude.Option.Some", [cid]]));
          });
        });
      }
      return;
    }
    if (ctx.EResult && ctx.EResult.isResult(val)) {
      if (ctx.EResult.isSuccess(val)) {
        const okHint = hk === "Result" ? hint.Result[1] : "Any";
        const childVal = val.success;
        work.push(() => {
          visit(childVal, okHint, (cid) => {
            put(emit(["tag", "Prelude.Result.Ok", [cid]]));
          });
        });
      } else {
        const errHint = hk === "Result" ? hint.Result[0] : "Any";
        const childVal = val.failure;
        work.push(() => {
          visit(childVal, errHint, (cid) => {
            put(emit(["tag", "Prelude.Result.Err", [cid]]));
          });
        });
      }
      return;
    }
    if (
      (ctx.Failure && val instanceof ctx.Failure) ||
      (val && typeof val === "object" && Array.isArray(val.reasons) && val.constructor?.name === "Cause")
    ) {
      const causeObj = ctx.Failure && val instanceof ctx.Failure ? val.cause : val;
      const reasons = causeObj?.reasons ?? [];
      const encoded = new Array(reasons.length);
      let i = 0;
      const next = () => {
        while (i < reasons.length) {
          const k = i++;
          const r = reasons[k];
          if (r._tag === "Fail") {
            work.push(next);
            visit(r.error, "Any", (cid) => {
              encoded[k] = ["fail", cid];
            });
            return;
          }
          if (r._tag === "Die") {
            const msg = r.defect instanceof Error ? r.defect.message : String(r.defect);
            encoded[k] = ["die", msg];
          } else {
            encoded[k] = ["interrupt", r.fiberId ?? null];
          }
        }
        put(emit(["cause", encoded]));
      };
      work.push(next);
      return;
    }

    if (Array.isArray(val)) {
      if (active.has(val)) {
        if (copy) throw new RangeError("Maximum call stack size exceeded");
        put(emit(["cycle", exec.retainHandle(val)]));
        return;
      }
      active.add(val);
      const isTuple = hk === "Tuple" || (hk === "Any" && tupleArrays.has(val));
      const itemHints = hk === "Tuple" ? hint.Tuple : null;
      const listElemHint = hk === "List" ? hint.List : "Any";
      const ids = new Array(val.length);
      let i = 0;
      const next = () => {
        if (i < val.length) {
          const k = i++;
          const child = val[k];
          const childHint = itemHints ? (itemHints[k] ?? "Any") : listElemHint;
          work.push(next);
          visit(child, childHint, (cid) => {
            ids[k] = cid;
          });
          return;
        }
        active.delete(val);
        put(emit([isTuple ? "tuple" : "list", ids]));
      };
      work.push(next);
      return;
    }

    const proto = Reflect.getPrototypeOf(val);
    if (proto !== Object.prototype && proto !== null) {
      put(emit(["handle", exec.retainHandle(val)]));
      return;
    }

    if (active.has(val)) {
      if (copy) throw new RangeError("Maximum call stack size exceeded");
      put(emit(["cycle", exec.retainHandle(val)]));
      return;
    }

    // Check Flick Option / Result / registered ADT tags safely via own data descriptors.
    const tagStr = ownStringProp(val, "_tag");
    if (tagStr !== undefined) {
      const ownKeys = Object.keys(val);
      if (tagStr === "None" && ownKeys.length === 1) {
        put(emit(["tag", "Prelude.Option.None", []]));
        return;
      }
      if (tagStr === "Some" && ownKeys.length === 2 && Object.hasOwn(val, "value")) {
        const innerHint = hk === "Option" ? hint.Option : "Any";
        active.add(val);
        const childVal = val.value;
        work.push(() => {
          visit(childVal, innerHint, (cid) => {
            active.delete(val);
            put(emit(["tag", "Prelude.Option.Some", [cid]]));
          });
        });
        return;
      }
      if (tagStr === "Success" && ownKeys.length === 2 && Object.hasOwn(val, "success")) {
        const okHint = hk === "Result" ? hint.Result[1] : "Any";
        active.add(val);
        const childVal = val.success;
        work.push(() => {
          visit(childVal, okHint, (cid) => {
            active.delete(val);
            put(emit(["tag", "Prelude.Result.Ok", [cid]]));
          });
        });
        return;
      }
      if (tagStr === "Failure" && ownKeys.length === 2 && Object.hasOwn(val, "failure")) {
        const errHint = hk === "Result" ? hint.Result[0] : "Any";
        active.add(val);
        const childVal = val.failure;
        work.push(() => {
          visit(childVal, errHint, (cid) => {
            active.delete(val);
            put(emit(["tag", "Prelude.Result.Err", [cid]]));
          });
        });
        return;
      }
      const fullTag =
        tagHashToFull.get(tagStr) ?? (/^[A-Z][^#]*#[0-9a-f]{12}$/.test(tagStr) ? tagStr : undefined);
      if (fullTag !== undefined) {
        active.add(val);
        const arity = ownKeys.length - 1;
        const fieldIds = new Array(arity);
        let i = 0;
        const next = () => {
          if (i < arity) {
            const k = i++;
            const child = val[`_${k}`];
            work.push(next);
            visit(child, "Any", (cid) => {
              fieldIds[k] = cid;
            });
            return;
          }
          active.delete(val);
          put(emit(["tag", fullTag, fieldIds]));
        };
        work.push(next);
        return;
      }
    }

    // Plain JS record object
    active.add(val);
    const fieldHintMap = hk === "Record" ? new Map(hint.Record) : null;
    const entries = Object.entries(val);
    const pairs = new Array(entries.length);
    let i = 0;
    const next = () => {
      if (i < entries.length) {
        const k = i++;
        const [key, child] = entries[k];
        const ch = fieldHintMap?.get(key) ?? "Any";
        work.push(next);
        visit(child, ch, (cid) => {
          pairs[k] = [key, cid];
        });
        return;
      }
      active.delete(val);
      put(emit(["rec", pairs]));
    };
    work.push(next);
  }

  visit(rootVal, rootHint, (id) => {
    rootIdx = id;
  });
  while (work.length > 0) {
    work.pop()();
  }
  return { root: rootIdx, nodes };
}
