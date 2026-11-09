// `Js.snapshot`: inert data copied without calling accessors. A proxy trap
// can still throw; callers translate that to a typed SnapshotError.
export class SnapshotRefused extends Error {}

const refuse = (path: string, reason: string): never => {
  throw new SnapshotRefused(`${path || "the value"}: ${reason}`);
};

const plain = (value: object): boolean => {
  const proto: unknown = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
};

const dataOf = (value: object, key: string | number, at: string, missing: string): PropertyDescriptor => {
  const descriptor = Object.getOwnPropertyDescriptor(value, key);
  if (descriptor === undefined) return refuse(at, missing);
  if (!("value" in descriptor)) return refuse(at, "an accessor");
  return descriptor;
};

/** Copies `root`, at most `depth` deep and `limit` values. */
export const snapshotCopy = (root: unknown, depth: number, limit: number): unknown => {
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
        const descriptor = dataOf(value, key, at, "a missing property");
        if (descriptor.enumerable) fields[key] = visit(descriptor.value, at, level + 1);
      }
      out = Object.freeze(fields);
    }
    active.delete(value);
    done.set(value, out);
    return out;
  };
  return visit(root, "", 0);
};

export const snapshotReason = (error: unknown): string =>
  error instanceof SnapshotRefused
    ? error.message
    : `reading the value threw: ${error instanceof Error ? error.message : "an exception"}`;
