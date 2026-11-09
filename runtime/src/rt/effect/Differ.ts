// JSON patch differ for Flick's pure codecs. Patches use RFC 6902 paths and
// apply in order, so a patch and its inverse can be combined.
import { equals } from "../../core/data.ts";
import { type Codec } from "../schema.ts";
export interface Patch { readonly op: "add" | "remove" | "replace"; readonly path: string; readonly value?: unknown }
export interface Differ<A> { readonly codec: Codec<A> }
const record = (value: unknown): value is Readonly<Record<string, unknown>> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const escape = (name: string): string => name.replaceAll("~", "~0").replaceAll("/", "~1");
const unescape = (name: string): string => name.replaceAll("~1", "/").replaceAll("~0", "~");
const diff = (oldValue: unknown, newValue: unknown, path: string): Array<Patch> => {
  if (equals(oldValue, newValue)) return [];
  if (Array.isArray(oldValue) && Array.isArray(newValue)) {
    const result: Array<Patch> = [];
    const common = Math.min(oldValue.length, newValue.length);
    for (let index = 0; index < common; index++) result.push(...diff(oldValue[index], newValue[index], `${path}/${index}`));
    for (let index = oldValue.length - 1; index >= newValue.length; index--) result.push({ op: "remove", path: `${path}/${index}` });
    for (let index = common; index < newValue.length; index++) result.push({ op: "add", path: `${path}/${index}`, value: newValue[index] });
    return result;
  }
  if (record(oldValue) && record(newValue)) {
    const result: Array<Patch> = [];
    for (const [name, value] of Object.entries(oldValue)) {
      const nextPath = `${path}/${escape(name)}`;
      if (!Object.hasOwn(newValue, name)) result.push({ op: "remove", path: nextPath });
      else result.push(...diff(value, newValue[name], nextPath));
    }
    for (const [name, value] of Object.entries(newValue)) {
      if (!Object.hasOwn(oldValue, name)) result.push({ op: "add", path: `${path}/${escape(name)}`, value });
    }
    return result;
  }
  return [{ op: "replace", path, value: newValue }];
};
const encoded = <A>(differ: Differ<A>, value: A): unknown => {
  const result = differ.codec.encode(value);
  if (result._tag === "Failure") throw new Error("cannot encode the value for JSON diff");
  return result.success;
};
export const flickEmpty = <A>(_differ: Differ<A>): Array<Patch> => [];
export const flickDiff = <A>(oldValue: A, newValue: A, differ: Differ<A>): Array<Patch> =>
  diff(encoded(differ, oldValue), encoded(differ, newValue), "");
export const flickCombine = <A>(first: ReadonlyArray<Patch>, second: ReadonlyArray<Patch>, _differ: Differ<A>): Array<Patch> =>
  [...first, ...second];
const apply = (source: unknown, patch: Patch): unknown => {
  if (patch.path === "") return patch.op === "remove" ? undefined : patch.value;
  const result: unknown = structuredClone(source);
  const segments = patch.path.slice(1).split("/").map(unescape);
  let parent: unknown = result;
  for (const segment of segments.slice(0, -1)) {
    if (Array.isArray(parent)) parent = parent[Number(segment)];
    else if (record(parent)) parent = parent[segment];
    else throw new Error("invalid JSON patch path");
  }
  const key = segments.at(-1) ?? "";
  if (Array.isArray(parent)) {
    const index = key === "-" ? parent.length : Number(key);
    if (patch.op === "remove") parent.splice(index, 1);
    else if (patch.op === "add") parent.splice(index, 0, patch.value);
    else parent[index] = patch.value;
  } else if (record(parent)) {
    if (patch.op === "remove") Reflect.deleteProperty(parent, key);
    else Reflect.set(parent, key, patch.value);
  } else throw new Error("invalid JSON patch path");
  return result;
};
export const flickPatch = <A>(oldValue: A, patches: ReadonlyArray<Patch>, differ: Differ<A>): A => {
  let value = encoded(differ, oldValue);
  for (const patch of patches) value = apply(value, patch);
  const result = differ.codec.decode(value);
  if (result._tag === "Failure") throw new Error("JSON patch produced an invalid value");
  return result.success;
};
