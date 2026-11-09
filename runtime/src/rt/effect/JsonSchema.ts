// Pure JSON Schema dialect conversion over JSON values. Definitions are kept
// beside the root as Flick's Document records require.
export const META_SCHEMA_URI_DRAFT_04 = "http://json-schema.org/draft-04/schema#";
export const META_SCHEMA_URI_DRAFT_07 = "http://json-schema.org/draft-07/schema#";
export const META_SCHEMA_URI_DRAFT_2020_12 = "https://json-schema.org/draft/2020-12/schema";
type Fields = Readonly<Record<string, unknown>>;
interface Document { readonly dialect: string; readonly root: unknown; readonly definitions: Fields }
interface MultiDocument { readonly dialect: string; readonly schemas: ReadonlyArray<unknown>; readonly definitions: Fields }
const record = (value: unknown): value is Fields => typeof value === "object" && value !== null && !Array.isArray(value);
const definitions = (value: unknown): Fields => record(value) ? value : {};
const rewrite = (value: unknown, from: string, to: string): unknown => {
  if (Array.isArray(value)) return value.map((item) => rewrite(item, from, to));
  if (!record(value)) return value;
  return Object.fromEntries(Object.entries(value).map(([key, item]) =>
    [key, key === "$ref" && typeof item === "string" && item.startsWith(from) ? `${to}${item.slice(from.length)}` : rewrite(item, from, to)]));
};
const without = (source: Fields, key: string): Fields => Object.fromEntries(Object.entries(source).filter(([name]) => name !== key));
export const flickFromSchemaDraft07 = (schema: unknown): Document => {
  const source = definitions(schema);
  return {
    dialect: "draft-2020-12",
    root: rewrite(without(source, "definitions"), "#/definitions/", "#/$defs/"),
    definitions: definitions(rewrite(source["definitions"], "#/definitions/", "#/$defs/")),
  };
};
export const flickFromSchemaDraft2020_12 = (schema: unknown): Document => {
  const source = definitions(schema);
  return { dialect: "draft-2020-12", root: without(source, "$defs"), definitions: definitions(source["$defs"]) };
};
export const flickFromSchemaOpenApi3_1 = (schema: unknown): Document =>
  ({ dialect: "draft-2020-12", root: schema, definitions: {} });
const nullable = (value: unknown): unknown => {
  if (Array.isArray(value)) return value.map(nullable);
  if (!record(value)) return value;
  const source = Object.fromEntries(Object.entries(value).filter(([key]) => key !== "nullable"));
  const type = source["type"];
  return value["nullable"] === true && typeof type === "string"
    ? { ...source, type: [type, "null"] } : Object.fromEntries(Object.entries(source).map(([key, item]) => [key, nullable(item)]));
};
export const flickFromSchemaOpenApi3_0 = (schema: unknown): Document =>
  ({ dialect: "draft-2020-12", root: nullable(schema), definitions: {} });
const draft = (document: Document, dialect: "draft-07" | "draft-04"): Document => {
  if (document.dialect !== "draft-2020-12") throw new RangeError(`expected a draft-2020-12 document, got ${document.dialect}`);
  const converted = rewrite(document.root, "#/$defs/", "#/definitions/");
  const root = record(converted) && typeof converted["$schema"] === "string"
    ? { ...converted, $schema: dialect === "draft-07" ? META_SCHEMA_URI_DRAFT_07 : META_SCHEMA_URI_DRAFT_04 }
    : converted;
  return { dialect, root, definitions: definitions(rewrite(document.definitions, "#/$defs/", "#/definitions/")) };
};
export const flickToDocumentDraft07 = (document: Document): Document => draft(document, "draft-07");
export const flickToDocumentDraft04 = (document: Document): Document => draft(document, "draft-04");
export const toMultiDocumentOpenApi3_1 = (document: MultiDocument): MultiDocument => ({
  dialect: "openapi-3.1",
  schemas: document.schemas.map((schema) => rewrite(schema, "#/$defs/", "#/components/schemas/")),
  definitions: definitions(rewrite(document.definitions, "#/$defs/", "#/components/schemas/")),
});
