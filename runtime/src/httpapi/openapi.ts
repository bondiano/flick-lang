// OpenAPI 3.1 of an `HttpApi`, with every schema written by the strict JSON
// Schema 2020-12 exporter: an exact wire contract, or an `ExportError`
// naming `group.endpoint.part -> field`. Nothing is approximated.
import { Result } from "effect";

import { exportError, exportJsonSchema, type ExportError } from "../schema/exporter.ts";
import type { AnySchema } from "../schema/schemas.ts";
import type { Api, Endpoint, Group, Security } from "./model.ts";

type Json = { readonly [key: string]: unknown };

/** The body of a request that does not decode: what the server answers with 400. */
export const DECODE_ERROR: Json = {
  type: "object",
  properties: {
    error: { const: "BadRequest" },
    issues: {
      type: "array",
      items: {
        type: "object",
        properties: { path: { type: "string" }, message: { type: "string" } },
        required: ["path", "message"],
        additionalProperties: false,
      },
    },
  },
  required: ["error", "issues"],
  additionalProperties: false,
};

class Refused extends Error {
  readonly error: ExportError;
  constructor(error: ExportError) {
    super(error._0);
    this.error = error;
  }
}

const isJson = (value: unknown): value is Json => typeof value === "object" && value !== null && !Array.isArray(value);

/** JSON text the exporter wrote, which is always an object. */
const parse = (text: string): Json => {
  const value: unknown = JSON.parse(text);
  if (!isJson(value)) throw new TypeError("the exporter writes an object");
  return value;
};

/** The schema of `codec`, its `$defs` moved into `components`. */
const schema = (codec: AnySchema, root: string, components: Map<string, unknown>): Json => {
  const result = exportJsonSchema(root, codec);
  if (Result.isFailure(result)) throw new Refused(result.failure);
  const { $schema: _, $defs, ...top } = parse(result.success);
  for (const [name, def] of Object.entries(isJson($defs) ? $defs : {})) {
    if (!components.has(name)) components.set(name, def);
  }
  return parse(JSON.stringify(top).replaceAll('"#/$defs/', '"#/components/schemas/'));
};

/** Path, query or header parameters from a record codec. */
const parameters = (
  codec: AnySchema | null,
  where: "path" | "query" | "header",
  root: string,
  components: Map<string, unknown>,
): Array<Json> => {
  if (codec === null) return [];
  const s = schema(codec, root, components);
  const properties = s["properties"];
  if (s["type"] !== "object" || !isJson(properties)) {
    throw new Refused(exportError(`${root} -> OpenAPI: parameters come from a record of fields`));
  }
  const listed: unknown = s["required"];
  const required = new Set(Array.isArray(listed) ? listed.filter((n): n is string => typeof n === "string") : []);
  return Object.entries(properties).map(([name, field]) => ({
    name,
    in: where,
    required: where === "path" || required.has(name),
    schema: field,
  }));
};

const json = (s: Json, type = "application/json"): Json => ({ content: { [type]: { schema: s } } });

const MEDIA = { json: "application/json", text: "text/plain", form: "application/x-www-form-urlencoded" } as const;

const operation = (group: Group, endpoint: Endpoint, components: Map<string, unknown>): Json => {
  const root = `${group.name}.${endpoint.name}`;
  const responses: Record<string, unknown> = {
    [String(endpoint.status)]:
      endpoint.success === null
        ? { description: "Success" }
        : {
            description: "Success",
            ...json(schema(endpoint.success, `${root}.success`, components), MEDIA[endpoint.successEncoding]),
          },
    "400": { description: "The request does not decode", ...json(DECODE_ERROR) },
  };
  const byStatus = new Map<number, Array<Json>>();
  for (const [i, e] of endpoint.errors.entries()) {
    byStatus.set(e.status, [...(byStatus.get(e.status) ?? []), schema(e.codec, `${root}.error${i}`, components)]);
  }
  for (const [status, schemas] of byStatus) {
    const one = schemas.length === 1 ? schemas[0] : { oneOf: schemas };
    responses[String(status)] = { description: "Error", ...json(one ?? {}) };
  }
  return {
    operationId: root,
    tags: [group.name],
    ...(endpoint.description === "" ? {} : { summary: endpoint.description }),
    ...(endpoint.deprecated ? { deprecated: true } : {}),
    ...(endpoint.security.length === 0 ? {} : { security: endpoint.security.map((s) => ({ [s.name]: [] })) }),
    parameters: [
      ...parameters(endpoint.params, "path", `${root}.path`, components),
      ...parameters(endpoint.query, "query", `${root}.query`, components),
      ...parameters(endpoint.headers, "header", `${root}.headers`, components),
    ],
    ...(endpoint.payload === null
      ? {}
      : {
          requestBody: {
            required: true,
            ...json(schema(endpoint.payload, `${root}.payload`, components), MEDIA[endpoint.payloadEncoding]),
          },
        }),
    responses,
  };
};

/** `/users/:id` as OpenAPI writes it: `/users/{id}`. */
const openApiPath = (path: string): string => path.replace(/:([A-Za-z_][\w]*)/g, "{$1}");

const securityScheme = (s: Security): Json => {
  if (s.kind === "bearer") return { type: "http", scheme: "bearer" };
  if (s.kind === "basic") return { type: "http", scheme: "basic" };
  return { type: "apiKey", name: s.key, in: s.in };
};

/** The document as a value; throws `Refused`. */
const document = (api: Api): Json => {
  const components = new Map<string, unknown>();
  const schemes = new Map<string, Json>();
  const paths: Record<string, Record<string, unknown>> = {};
  for (const group of api.groups) {
    for (const endpoint of group.endpoints.filter((e) => !e.hidden)) {
      for (const s of endpoint.security) schemes.set(s.name, securityScheme(s));
      const path = openApiPath(endpoint.path);
      paths[path] = { ...paths[path], [endpoint.method.toLowerCase()]: operation(group, endpoint, components) };
    }
  }
  const component = {
    ...(components.size > 0 ? { schemas: Object.fromEntries(components) } : {}),
    ...(schemes.size > 0 ? { securitySchemes: Object.fromEntries(schemes) } : {}),
  };
  return {
    openapi: "3.1.0",
    info: {
      title: api.name,
      version: api.info.version,
      ...(api.info.description === "" ? {} : { description: api.info.description }),
    },
    ...(api.info.servers.length === 0 ? {} : { servers: api.info.servers.map((url) => ({ url })) }),
    tags: api.groups.map((g) => ({ name: g.name })),
    paths,
    ...(Object.keys(component).length > 0 ? { components: component } : {}),
  };
};

/** `HttpApi.openapi`: the document as JSON text. */
export const httpApiOpenApi = (api: Api): Result.Result<string, ExportError> => {
  try {
    return Result.succeed(JSON.stringify(document(api), null, 2));
  } catch (error) {
    if (error instanceof Refused) return Result.fail(error.error);
    throw error;
  }
};
