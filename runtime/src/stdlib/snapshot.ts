// `Js.snapshot`: a copy of a JS value as inert data. Reading a property
// through its descriptor never calls a getter; a Proxy trap may still run,
// so a throw while reading is a typed error, not a crash.
import { Effect } from "effect";

import type { Tagged } from "../data.ts";
import { tag } from "../tags.ts";

const SNAPSHOT = tag("Js.SnapshotError.SnapshotError");

/** `Js.SnapshotError`. */
export type SnapshotError = Tagged<typeof SNAPSHOT, string>;

class Refused extends Error {}

const refuse = (path: string, reason: string): never => {
  throw new Refused(`${path || "the value"}: ${reason}`);
};

const plain = (value: object): boolean => {
  const proto: unknown = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
};

/** The descriptor of a data property; an accessor or a hole is refused. */
const dataOf = (value: object, key: string | number, at: string, missing: string): PropertyDescriptor => {
  const d = Object.getOwnPropertyDescriptor(value, key);
  if (d === undefined) return refuse(at, missing);
  if (!("value" in d)) return refuse(at, "an accessor");
  return d;
};

/** Copies `root`, at most `depth` deep and `limit` values. */
const copy = (root: unknown, depth: number, limit: number): unknown => {
  const done = new Map<object, unknown>();
  const active = new Set<object>();
  let count = 0;
  const visit = (value: unknown, path: string, level: number): unknown => {
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
    let out: unknown;
    if (array) {
      const items: Array<unknown> = [];
      for (let i = 0; i < value.length; i++) {
        const at = `${path}[${i}]`;
        items.push(visit(dataOf(value, i, at, "a hole in an array").value, at, level + 1));
      }
      out = Object.freeze(items);
    } else {
      const fields: Record<string, unknown> = {};
      for (const key of Reflect.ownKeys(value)) {
        if (typeof key === "symbol") return refuse(path, "a symbol key");
        const at = `${path}.${key}`;
        const d = dataOf(value, key, at, "a missing property");
        if (d.enumerable) fields[key] = visit(d.value, at, level + 1);
      }
      out = Object.freeze(fields);
    }
    active.delete(value);
    done.set(value, out);
    return out;
  };
  return visit(root, "", 0);
};

const messageOf = (error: unknown): string =>
  error instanceof Error ? error.message : "an exception";

/** `Js.snapshot_with depth limit value`. */
export const snapshot = (depth: number, limit: number, value: unknown): Effect.Effect<unknown, SnapshotError> =>
  Effect.suspend(() => {
    try {
      return Effect.succeed(copy(value, depth, limit));
    } catch (error) {
      const reason = error instanceof Refused ? error.message : `reading the value threw: ${messageOf(error)}`;
      return Effect.fail({ _tag: SNAPSHOT, _0: reason });
    }
  });

export const snapshotDefault = (value: unknown): Effect.Effect<unknown, SnapshotError> => snapshot(64, 100_000, value);
