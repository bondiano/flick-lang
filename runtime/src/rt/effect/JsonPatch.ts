// RFC 6902's add/remove/replace subset over plain JSON values.
import { err, ok, type Result } from "../../core/data.ts";
import { tag, constructorName } from "../../tags.ts";
import { flickDiff, type Patch } from "./Differ.ts";

type Operation = Readonly<{ _tag: string; _0: string; _1?: unknown }>;
const ADD = tag("JsonPatch.Operation.Add");
const REMOVE = tag("JsonPatch.Operation.Remove");
const REPLACE = tag("JsonPatch.Operation.Replace");
const PATCH_ERROR = tag("JsonPatch.PatchError.PatchError");
const record = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const unescape = (name: string): string => name.replaceAll("~1", "/").replaceAll("~0", "~");
const same = { encode: (value: unknown) => ok(value), decode: (value: unknown) => ok(value) };
const differ = { codec: { kind: "json", ...same } };
export const flickGet = (before: unknown, after: unknown): Array<Operation> =>
  flickDiff(before, after, differ).map((patch) => patch.op === "remove"
    ? { _tag: REMOVE, _0: patch.path }
    : { _tag: patch.op === "add" ? ADD : REPLACE, _0: patch.path, _1: patch.value });
const fail = (action: string, path: string, why: string): never =>
  { throw new Error(`Cannot ${action} at ${JSON.stringify(path)} (${why}).`); };
const apply = (source: unknown, patch: Patch): unknown => {
  const action = patch.op;
  if (patch.path === "") {
    if (action === "remove") fail(action, patch.path, "cannot remove the document root");
    return patch.value;
  }
  if (!patch.path.startsWith("/")) fail(action, patch.path, "invalid JSON Pointer");
  const copy: unknown = structuredClone(source);
  const segments = patch.path.slice(1).split("/").map(unescape);
  let parent: unknown = copy;
  for (const segment of segments.slice(0, -1)) {
    if (Array.isArray(parent)) {
      const index = Number(segment);
      parent = Number.isInteger(index) && index >= 0 && index < parent.length ? parent[index] : undefined;
    } else if (record(parent)) parent = Object.hasOwn(parent, segment) ? parent[segment] : undefined;
    else parent = undefined;
    if (parent === undefined) fail(action, patch.path, "parent not found or not a container");
  }
  const key = segments.at(-1) ?? "";
  if (Array.isArray(parent)) {
    const index = key === "-" && action === "add" ? parent.length : Number(key);
    if (!Number.isInteger(index) || index < 0 || index > parent.length || (action !== "add" && index === parent.length))
      fail(action, patch.path, "array index out of bounds");
    if (action === "remove") parent.splice(index, 1);
    else if (action === "add") parent.splice(index, 0, patch.value);
    else parent[index] = patch.value;
  } else if (record(parent)) {
    if (action !== "add" && !Object.hasOwn(parent, key)) fail(action, patch.path, "key not found");
    if (action === "remove") Reflect.deleteProperty(parent, key);
    else parent[key] = patch.value;
  } else fail(action, patch.path, "parent not found or not a container");
  return copy;
};
export const flickApply = (operations: ReadonlyArray<Operation>, source: unknown): Result<unknown, Readonly<{ _tag: string; _0: string }>> => {
  try {
    let value = source;
    for (const operation of operations) {
      const name = constructorName(operation._tag);
      const op: Patch = { op: name === "Add" ? "add" : name === "Remove" ? "remove" : "replace", path: operation._0, value: operation._1 };
      value = apply(value, op);
    }
    return ok(value);
  } catch (error) {
    return err({ _tag: PATCH_ERROR, _0: error instanceof Error ? error.message : String(error) });
  }
};
