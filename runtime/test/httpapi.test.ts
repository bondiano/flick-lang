import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { Ajv2020 } from "ajv/dist/2020.js";
import { Result } from "effect";

import {
  httpApiAdd,
  httpApiAddGroup,
  httpApiEndpoint,
  httpApiErrorCase,
  httpApiGroup,
  httpApiMake,
  httpApiOpenApi,
  httpApiWithErrors,
  httpApiWithPath,
  httpApiWithPayload,
  httpApiWithQuery,
  httpApiWithSuccess,
} from "../src/httpapi/index.ts";
import { schemaJsonCodec, schemaNumber, schemaRecord, schemaString, schemaUnknown } from "../src/index.ts";

const json = (fields: ReadonlyArray<readonly [string, typeof schemaString]>) => schemaJsonCodec(schemaRecord(fields));
const get = { _tag: "Get#0" };
const post = { _tag: "Post#0" };

const api = (payload: typeof schemaString) => {
  const user = json([["id", schemaString], ["name", schemaString]]);
  const getUser = httpApiWithErrors(
    [httpApiErrorCase(404, json([["id", schemaString]]))],
    httpApiWithSuccess(200, user, httpApiWithQuery(json([["limit", schemaNumber]]), httpApiWithPath(json([["id", schemaString]]), httpApiEndpoint(get, "/users/:id", "get_user")))),
  );
  const create = httpApiWithSuccess(201, user, httpApiWithPayload(payload, httpApiEndpoint(post, "/users", "create_user")));
  const group = httpApiAdd([], [], create, httpApiAdd([], [], getUser, httpApiGroup("users")));
  return httpApiAddGroup([], [], group, httpApiMake("Users"));
};

test("the OpenAPI document is valid OpenAPI 3.1 and writes no empty schema", () => {
  const result = httpApiOpenApi(api(json([["name", schemaString]])));
  assert.ok(Result.isSuccess(result));
  const document: unknown = JSON.parse(result.success);
  // Ajv resolves `$dynamicRef: "#meta"` against the wrong anchor; in the base
  // dialect it is the schema of a Schema Object, which `$ref` names exactly
  const text = readFileSync(new URL("./fixtures/openapi-3.1.schema.json", import.meta.url), "utf8");
  const meta: unknown = JSON.parse(text.replaceAll('"$dynamicRef": "#meta"', '"$ref": "#/$defs/schema"'));
  assert.ok(typeof meta === "object" && meta !== null);
  const ajv = new Ajv2020({ strict: false });
  const valid = ajv.validate(meta, document);
  assert.ok(valid, JSON.stringify(ajv.errors));
  assert.ok(!result.success.includes("{}") && !/"schema": true/.test(result.success));
  assert.match(result.success, /"\/users\/\{id\}"/);
});

test("a field without a strict form is an ExportError naming group, endpoint and field", () => {
  const result = httpApiOpenApi(api(json([["data", schemaUnknown]])));
  assert.ok(Result.isFailure(result));
  assert.match(result.failure._0, /^users\.create_user\.payload\.data -> JSON Schema: /);
});
