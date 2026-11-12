/* oxlint-disable typescript/no-unsafe-type-assertion -- the messages are the JSON the protocol writes */
// The RPC client of Flick's own runtime against the answers of Effect's RPC
// server, as it sends them (captured from `effect/rpc` 4.1 serving the same
// procedures): one `Request` per POST, the call's messages back.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { test } from "node:test";

import { err, ok } from "../src/core/data.ts";
import {
  type Procedure,
  rpcCall,
  rpcCallNdjson,
  rpcErrorCase,
  rpcErrors,
  rpcHttp,
  rpcJson,
  rpcMake,
  rpcWithErrors,
  rpcWithPayload,
  rpcWithSuccess,
} from "../src/rt/effect/rpc/Rpc.ts";
import { type Codec, schemaError, schemaField, schemaNumber, schemaRecordOf, schemaString } from "../src/rt/schema.ts";
import type { Exit } from "../src/rt/kernel.ts";
import { root } from "./rt-support.ts";

const notFound: Codec = {
  kind: "not-found",
  decode: (v) => (typeof v === "object" && v !== null && Reflect.get(v, "_tag") === "NotFound" ? ok(v) : err(schemaError("", "not NotFound"))),
  encode: (v) => ok(v),
};

const getUser: Procedure = rpcWithErrors(
  rpcErrorCase(notFound, rpcErrors),
  rpcWithSuccess(schemaRecordOf([schemaField("id", schemaNumber), schemaField("name", schemaString)]), rpcWithPayload(schemaNumber, rpcMake("get_user"))),
);
const ping: Procedure = rpcMake("ping");

/** A `Request` message as the client writes it. */
interface Sent {
  readonly _tag: string;
  readonly id: number;
  readonly tag: string;
  readonly payload: unknown;
  readonly headers: unknown;
  readonly traceId: string;
  readonly spanId: string;
}

interface Seen {
  readonly contentType: string | undefined;
  readonly body: string;
}

/** A server that answers each request with `answer(request)`, written in `json` or as lines. */
const serve = async (answer: (request: Sent) => ReadonlyArray<unknown> | string, lines = true) => {
  const seen: Array<Seen> = [];
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (chunk: Buffer) => (body += chunk.toString()));
    req.on("end", () => {
      seen.push({ contentType: req.headers["content-type"], body });
      const reply = answer(JSON.parse(body) as Sent);
      if (typeof reply === "string") res.end(reply);
      else res.end(lines ? reply.map((m) => `${JSON.stringify(m)}\n`).join("") : JSON.stringify(reply));
    });
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address() as AddressInfo;
  return { url: `http://127.0.0.1:${port}/rpc`, seen, close: () => new Promise<void>((resolve) => server.close(() => resolve())) };
};

const users = (request: Sent): ReadonlyArray<unknown> => {
  const requestId = request.id;
  if (request.tag === "ping") return [{ _tag: "Exit", requestId, exit: { _tag: "Success", value: null } }];
  if (request.payload === 1) return [{ _tag: "Exit", requestId, exit: { _tag: "Success", value: { id: 1, name: "Ada" } } }];
  if (request.payload === 2) return [{ _tag: "Defect", defect: "boom" }];
  return [{ _tag: "Exit", requestId, exit: { _tag: "Failure", cause: [{ _tag: "Fail", error: { _tag: "NotFound", _0: { id: request.payload } } }] } }];
};

test("an NDJSON call sends one request line and decodes its success, error or defect", async () => {
  const server = await serve(users);
  try {
    const ada = await root(() => rpcCallNdjson(server.url, getUser, 1));
    assert.deepEqual(ada.value, { id: 1, name: "Ada" });
    const [first] = server.seen;
    assert.equal(first?.contentType, "application/x-ndjson");
    assert.ok(first?.body.endsWith("}\n"));
    const request = JSON.parse(first?.body ?? "") as Sent;
    assert.equal(request._tag, "Request");
    assert.equal(request.tag, "get_user");
    assert.equal(request.payload, 1);
    assert.deepEqual(request.headers, []);
    assert.match(request.traceId, /^[0-9a-f]{32}$/);
    assert.match(request.spanId, /^[0-9a-f]{16}$/);

    const missing = await root(() => rpcCallNdjson(server.url, getUser, 9));
    assert.equal(missing.cause?.reasons[0]?._tag, "Fail");
    assert.deepEqual(missing.cause?.reasons[0], { _tag: "Fail", error: { _tag: "NotFound", _0: { id: 9 } } });

    const boom = await root(() => rpcCallNdjson(server.url, getUser, 2));
    assert.deepEqual(boom.cause?.reasons, [{ _tag: "Die", defect: "boom" }]);
  } finally {
    await server.close();
  }
});

test("a procedure without a payload sends null, and its success is Unit", async () => {
  const server = await serve(users);
  try {
    const pinged = await root(() => rpcCallNdjson(server.url, ping, undefined));
    assert.equal(pinged.cause, undefined);
    assert.equal(pinged.value, undefined);
    assert.equal((JSON.parse(server.seen[0]?.body ?? "") as Sent).payload, null);
  } finally {
    await server.close();
  }
});

test("a JSON call sends the request alone and reads the messages as one array", async () => {
  const server = await serve(users, false);
  try {
    const ada = await root(() => rpcCall({ serialization: rpcJson, transport: rpcHttp }, server.url, getUser, 1));
    assert.deepEqual(ada.value, { id: 1, name: "Ada" });
    assert.equal(server.seen[0]?.contentType, "application/json");
    assert.ok(server.seen[0]?.body.endsWith("}"));
  } finally {
    await server.close();
  }
});

const clientError = (exit: Exit): string => {
  const reason = exit.cause?.reasons[0] as { readonly _tag: string; readonly error: { readonly _tag: string; readonly _0: string } };
  assert.equal(reason._tag, "Fail");
  assert.match(reason.error._tag, /^ClientError#/);
  return reason.error._0;
};

test("no server, no answer or no RPC answer is a ClientError", async () => {
  const absent = await root(() => rpcCallNdjson("http://127.0.0.1:9/rpc", getUser, 1));
  assert.match(clientError(absent), /^HttpError: /);

  const silent = await serve(() => []);
  const nothing = await serve(() => "<html>not here</html>");
  const other = await serve((request) => [{ _tag: "Exit", requestId: request.id + 1, exit: { _tag: "Success", value: null } }]);
  try {
    assert.equal(clientError(await root(() => rpcCallNdjson(silent.url, getUser, 1))), "RpcClientDefect: Received empty HTTP response from RPC server");
    assert.equal(clientError(await root(() => rpcCallNdjson(nothing.url, getUser, 1))), "RpcClientDefect: Error decoding HTTP response");
    assert.equal(clientError(await root(() => rpcCallNdjson(other.url, getUser, 1))), "RpcClientDefect: HTTP response ended before RPC request completed");
  } finally {
    await Promise.all([silent.close(), nothing.close(), other.close()]);
  }
});
