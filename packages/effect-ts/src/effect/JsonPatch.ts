// Effect's `JsonPatch` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: an operation
// is a `JsonPatch.Operation`, and a patch that cannot apply is an error.
import { JsonPatch, type Result, type Schema } from "effect";

import { type Tagged, err, ok } from "../data.ts";
import { constructorName, tag } from "@flick-lang/runtime/tags";

export * from "effect/JsonPatch";

const ADD = /*#__PURE__*/ tag("JsonPatch.Operation.Add");
const REMOVE = /*#__PURE__*/ tag("JsonPatch.Operation.Remove");
const REPLACE = /*#__PURE__*/ tag("JsonPatch.Operation.Replace");
const PATCH_ERROR = /*#__PURE__*/ tag("JsonPatch.PatchError.PatchError");

/** `JsonPatch.Operation`: a path, and a value but for `Remove`. */
type Operation = Readonly<{ _tag: string; _0: string; _1?: Schema.Json }>;

const toFlick = (op: JsonPatch.JsonPatchOperation): Operation =>
  op.op === "remove"
    ? { _tag: REMOVE, _0: op.path }
    : { _tag: op.op === "add" ? ADD : REPLACE, _0: op.path, _1: op.value };

const toEffect = (op: Operation): JsonPatch.JsonPatchOperation => {
  const value = op._1 ?? null;
  switch (constructorName(op._tag)) {
    case "Remove":
      return { op: "remove", path: op._0 };
    case "Add":
      return { op: "add", path: op._0, value };
    default:
      return { op: "replace", path: op._0, value };
  }
};

/** `JsonPatch.get old new`. */
export const flickGet = (oldValue: Schema.Json, newValue: Schema.Json): Array<Operation> =>
  JsonPatch.get(oldValue, newValue).map(toFlick);

/** `JsonPatch.apply patch old`: a throw of Effect's is a `PatchError`. */
export const flickApply = (
  patch: ReadonlyArray<Operation>,
  oldValue: Schema.Json,
): Result.Result<Schema.Json, Tagged<typeof PATCH_ERROR, string>> => {
  try {
    return ok(JsonPatch.apply(patch.map(toEffect), oldValue));
  } catch (e) {
    return err({ _tag: PATCH_ERROR, _0: e instanceof Error ? e.message : String(e) });
  }
};
