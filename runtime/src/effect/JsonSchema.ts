// Effect's `JsonSchema` for the bindings of the standard library, and the
// adapters of its documents: `schema` is a keyword of Flick, so a
// `JsonSchema.Document` names the schema `root`.
import { JsonSchema } from "effect";

export * from "effect/JsonSchema";

/** `JsonSchema.Document`. */
interface Document {
  readonly dialect: JsonSchema.Dialect;
  readonly root: JsonSchema.JsonSchema;
  readonly definitions: JsonSchema.Definitions;
}

const toFlick = (d: JsonSchema.Document<JsonSchema.Dialect>): Document =>
  Object.freeze({ dialect: d.dialect, root: d.schema, definitions: d.definitions });

/** A document of draft 2020-12; another dialect is a defect of the caller. */
const draft2020 = (d: Document): JsonSchema.Document<"draft-2020-12"> => {
  if (d.dialect !== "draft-2020-12") throw new RangeError(`expected a draft-2020-12 document, got ${d.dialect}`);
  return { dialect: "draft-2020-12", schema: d.root, definitions: d.definitions };
};

export const flickFromSchemaDraft07 = (js: JsonSchema.JsonSchema): Document => toFlick(JsonSchema.fromSchemaDraft07(js));
export const flickFromSchemaDraft2020_12 = (js: JsonSchema.JsonSchema): Document =>
  toFlick(JsonSchema.fromSchemaDraft2020_12(js));
export const flickFromSchemaOpenApi3_1 = (js: JsonSchema.JsonSchema): Document => toFlick(JsonSchema.fromSchemaOpenApi3_1(js));
export const flickFromSchemaOpenApi3_0 = (js: JsonSchema.JsonSchema): Document => toFlick(JsonSchema.fromSchemaOpenApi3_0(js));
export const flickToDocumentDraft07 = (d: Document): Document => toFlick(JsonSchema.toDocumentDraft07(draft2020(d)));
export const flickToDocumentDraft04 = (d: Document): Document => toFlick(JsonSchema.toDocumentDraft04(draft2020(d)));
