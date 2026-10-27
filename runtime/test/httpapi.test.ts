import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { Ajv2020 } from "ajv/dist/2020.js";
import { Effect, Option, Redacted, Result } from "effect";

import {
  httpApiAdd,
  httpApiAddGroup,
  httpApiCall,
  httpApiEndpoint,
  httpApiErrorCase,
  httpApiErrors,
  httpApiGroup,
  httpApiKey,
  httpApiMake,
  httpApiOpenApi,
  httpApiSecuritySetCookie,
  httpApiWithErrors,
  httpApiWithPath,
  httpApiWithPayload,
  httpApiWithQuery,
  httpApiWithSuccess,
  httpApiWithTextSuccess,
} from "../src/httpapi/index.ts";
import { schemaJsonCodec, schemaNumber, schemaRecord, schemaString, schemaUnknown } from "../src/index.ts";

const json = (fields: ReadonlyArray<readonly [string, typeof schemaString]>) => schemaJsonCodec(schemaRecord(fields));
const get = { _tag: "Get#0" };
const post = { _tag: "Post#0" };

const api = (payload: typeof schemaString) => {
  const user = json([["id", schemaString], ["name", schemaString]]);
  const getUser = httpApiWithErrors(
    httpApiErrorCase(404, json([["id", schemaString]]), httpApiErrors),
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

const respond = (headers: HeadersInit): void => {
  globalThis.fetch = () => Promise.resolve(new Response("hi", { status: 200, headers }));
};

/** The text and the cookie a login `call` succeeds with. */
const loggedIn = (endpoint: Parameters<typeof httpApiCall>[1]) => {
  const request = { path: undefined, query: undefined, headers: undefined, payload: undefined };
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a text success with a cookie, needing no service
  const call = httpApiCall("http://api.test", endpoint, request) as Effect.Effect<readonly [string, Option.Option<Redacted.Redacted>]>;
  return Effect.runPromise(call);
};

test("a login whose cookie the response hides, as a browser hides HttpOnly ones, succeeds without it", async () => {
  const session = httpApiKey("session", { key: "sid", location: { _tag: "InCookie#0" } });
  const login = httpApiSecuritySetCookie(session, httpApiWithTextSuccess(200, schemaString, httpApiEndpoint(post, "/login", "login")));
  const original = globalThis.fetch;
  try {
    respond({});
    const [greeting, hidden] = await loggedIn(login);
    assert.equal(greeting, "hi");
    assert.ok(Option.isNone(hidden));
    respond({ "set-cookie": "sid=s3cret; Secure; HttpOnly" });
    const [, shown] = await loggedIn(login);
    assert.equal(Option.map(shown, Redacted.value).pipe(Option.getOrUndefined), "s3cret");
  } finally {
    globalThis.fetch = original;
  }
});
