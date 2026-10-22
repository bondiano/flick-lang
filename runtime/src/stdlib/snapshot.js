// `Js.snapshot`: a copy of a JS value as inert data. Reading a property
// through its descriptor never calls a getter; a Proxy trap may still run,
// so a throw while reading is a typed error, not a crash.
import { Effect } from "effect";

const SNAPSHOT = "Js.SnapshotError.SnapshotError";

class Refused extends Error {}

const refuse = (path, reason) => {
  throw new Refused(`${path || "the value"}: ${reason}`);
};

const plain = (value) => {
  const proto = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
};

/** Copies `root`, at most `depth` deep and `limit` values. */
const copy = (root, depth, limit) => {
  const done = new Map();
  const active = new Set();
  let count = 0;
  const visit = (value, path, level) => {
    if (++count > limit) refuse(path, `more than ${limit} values`);
    switch (typeof value) {
      case "function":
        return refuse(path, "a function is not data");
      case "symbol":
        return refuse(path, "a symbol is not data");
      case "object":
        break;
      default:
        return value;
    }
    if (value === null) return null;
    if (done.has(value)) return done.get(value);
    if (active.has(value)) refuse(path, "a cycle");
    if (level >= depth) refuse(path, `deeper than ${depth}`);
    const array = Array.isArray(value);
    if (!array && !plain(value)) refuse(path, "an object with another prototype");
    active.add(value);
    const keys = Reflect.ownKeys(value);
    let out;
    if (array) {
      out = new Array(value.length);
      for (let i = 0; i < value.length; i++) {
        const d = Object.getOwnPropertyDescriptor(value, i);
        if (d === undefined) refuse(`${path}[${i}]`, "a hole in an array");
        if (!("value" in d)) refuse(`${path}[${i}]`, "an accessor");
        out[i] = visit(d.value, `${path}[${i}]`, level + 1);
      }
    } else {
      out = {};
      for (const key of keys) {
        if (typeof key === "symbol") refuse(path, "a symbol key");
        const d = Object.getOwnPropertyDescriptor(value, key);
        const at = `${path}.${key}`;
        if (!("value" in d)) refuse(at, "an accessor");
        if (d.enumerable) out[key] = visit(d.value, at, level + 1);
      }
    }
    active.delete(value);
    Object.freeze(out);
    done.set(value, out);
    return out;
  };
  return visit(root, "", 0);
};

/** `Js.snapshot_with depth limit value`. */
export const snapshot = (depth, limit, value) =>
  Effect.suspend(() => {
    try {
      return Effect.succeed(copy(value, depth, limit));
    } catch (error) {
      const reason =
        error instanceof Refused ? error.message : `reading the value threw: ${String(error?.message ?? "an exception")}`;
      return Effect.fail({ _tag: SNAPSHOT, _0: reason });
    }
  });

export const snapshotDefault = (value) => snapshot(64, 100_000, value);
