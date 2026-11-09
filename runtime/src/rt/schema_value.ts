// Runtime validation walks value witnesses; JSON encoding is a separate check.
import { err, ok, type Result } from "../core/data.ts";
import type { SchemaError } from "./schema.ts";
import type { SchemaNode } from "./schema_nodes.ts";

const record = (value: unknown): value is Readonly<Record<string, unknown>> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const at = (path: string, key: string | number): string =>
  typeof key === "number" ? `${path}[${key}]` : path ? `${path}.${key}` : key;

export const structuralValue = (
  node: SchemaNode | undefined, input: unknown, path: string,
  issue: (path: string, message: string) => SchemaError,
): Result<unknown, SchemaError> | undefined => {
  if (node === undefined) return undefined;
  switch (node.kind) {
    case "list": case "chunk":
      if (!Array.isArray(input)) return undefined;
      for (let i = 0; i < input.length; i++) {
        const checked = node.item.value(input[i], at(path, i));
        if (checked._tag === "Failure") return checked;
      }
      break;
    case "tuple":
      if (!Array.isArray(input)) return undefined;
      for (const [i, item] of node.items.entries()) {
        const checked = item.value(input[i], at(path, i));
        if (checked._tag === "Failure") return checked;
      }
      break;
    case "record":
      if (!record(input)) return undefined;
      for (const [name, item] of node.fields) {
        if (!Object.hasOwn(input, name)) return err(issue(at(path, name), "a missing key"));
        const checked = item.value(input[name], at(path, name));
        if (checked._tag === "Failure") return checked;
      }
      break;
    case "adt": {
      if (!record(input)) return undefined;
      const selected = node.cases.find((item) => item.tag === input["_tag"]);
      if (selected === undefined) return undefined;
      for (const [i, item] of selected.fields.entries()) {
        const key = `_${i}`;
        const checked = item.value(input[key], at(path, key));
        if (checked._tag === "Failure") return checked;
      }
      break;
    }
    case "option":
      if (!record(input)) return undefined;
      if (input["_tag"] === "Some") {
        const checked = node.item.value(input["value"], path);
        if (checked._tag === "Failure") return checked;
      } else if (input["_tag"] !== "None") return undefined;
      break;
    case "result": {
      if (!record(input)) return undefined;
      const success = input["_tag"] === "Success";
      if (!success && input["_tag"] !== "Failure") return undefined;
      const checked = (success ? node.value : node.error).value(input[success ? "success" : "failure"], at(path, success ? "ok" : "err"));
      if (checked._tag === "Failure") return checked;
      break;
    }
    case "lazy": return node.get().value(input, path);
    default: return undefined;
  }
  return ok(input);
};
