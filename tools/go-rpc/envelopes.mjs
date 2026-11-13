// Refresh transport envelopes against the pinned Effect implementation.
// Schema-specific scalar vectors remain independent legacy vectors.
import { readFileSync, writeFileSync } from "node:fs";
import * as Binary from "../../packages/effect-ts/node_modules/effect/dist/encoding/SchemaBinary.js";
import * as Message from "../../packages/effect-ts/node_modules/effect/dist/rpc/RpcMessage.js";
const file = new URL("../../runtime-go/flickrt/rpc_binary_vectors_test.go", import.meta.url);
const encoder = Binary.encoder(Message.EncodedSchema, { fingerprint: true });
const bytes = (message) => {
  const out = { ...message };
  for (const key of ["payload", "values", "exit", "defect"]) {
    if (typeof out[key] === "string") out[key] = new Uint8Array(Buffer.from(out[key], "hex"));
  }
  return out;
};
const source = readFileSync(file, "utf8");
const updated = source.replace(/(\{"msgs?\/[^"\n]+", `([^`]+)`, )"[a-f0-9]+"/g, (_, prefix, tree) => {
  const input = JSON.parse(tree);
  const encoded = Array.isArray(input) ? encoder.encodeMany(input.map(bytes)) : encoder.encode(bytes(input));
  return `${prefix}"${Buffer.from(encoded).toString("hex")}"`;
});
writeFileSync(file, updated);

// Additional layouts and evolution frames. These bytes come only from
// Effect's independent implementation, not the Go writer being tested.
const Schema = await import("../../packages/effect-ts/node_modules/effect/dist/Schema.js");
const encoded = (schema, value) => Buffer.from(Binary.encoder(Schema.toEncoded(schema)).encode(value)).toString("hex");
const tree = Schema.toCodecStringTree;
const Adapter = await import("../../packages/effect-ts/dist/effect/Schema.js");
const Source = await import("../../packages/effect-ts/dist/schema/index.js");
const structuredJson = Source.schemaJsonCodec(Adapter.flickToEncoded(Adapter.flickEntries(Schema.String)));
const flippedStructured = Schema.flip(Adapter.flickFromJsonString(structuredJson));
const member = (tag) => Schema.Struct({ _tag: Schema.tag(tag), n: Schema.Number });
const union = Schema.Union([member("A"), member("B")]);
const vectors = {
  "flipped structured Json": encoded(flippedStructured, { a: "x", b: "y" }),
  "literal block": encoded(member("A"), { _tag: "A", n: 2 }),
  "tagged codec union": encoded(union, { _tag: "A", n: 2 }),
  "string record": encoded(Schema.Record(Schema.String, Schema.String), { a: "x", b: "y" }),
  "entries": encoded(Schema.Record(Schema.String, Schema.Number), { a: 1, b: 2 }),
  "bool tree true": encoded(tree(Schema.Boolean), "true"),
  "bool tree false": encoded(tree(Schema.Boolean), "false"),
  "bigint tree": encoded(tree(Schema.BigInt), "42"),
  "record tree": encoded(tree(Schema.Struct({ s: Schema.String })), { s: "x" }),
  "unknown union field": encoded(Schema.Struct({ maybe: Schema.optional(union) }), { maybe: { _tag: "B", n: 2 } }),
  "unknown union element": encoded(Schema.Array(union), [{ _tag: "B", n: 2 }]),
};
writeFileSync(new URL("../../runtime-go/flickrt/testdata/rpc_binary.json", import.meta.url), JSON.stringify(vectors, null, 2) + "\n");

// Use the actual Flick adapter witnesses: its custom int32 filter is not
// Effect's built-in isInt32 check, so their string-tree layouts differ.
const { schemaInt, schemaNumber, schemaString, schemaResult } = await import("../../packages/effect-ts/dist/schema/schemas.js");
for (const schema of [tree(schemaInt), tree(schemaNumber), Source.schemaJsonCodec(schemaResult(schemaString, schemaNumber))]) {
  try {
    Binary.encoder(Schema.toEncoded(schema));
  } catch (error) {
    if (error.message.includes("union members are not uniquely identifiable")) continue;
    throw error;
  }
  throw new Error("expected ambiguous Flick binary layout to be rejected");
}

// The pinned serializer's error/reset and request-flag behavior is part of
// the transport contract as well as its binary layout.
const Serialization = await import("../../packages/effect-ts/node_modules/effect/dist/rpc/RpcSerialization.js");
const ndjson = Serialization.makeNdjson().makeUnsafe();
let malformedRejected = false;
try { ndjson.decode("broken\n"); } catch (error) {
  if (error._tag !== "NdjsonDecodeError" || error.cause.name !== "SyntaxError") throw error;
  malformedRejected = true;
}
if (!malformedRejected || ndjson.decode('{"_tag":"Pong"}\n')[0]?._tag !== "Pong") throw new Error("NDJSON must reject malformed frames and reset");
if (Serialization.makeNdjson({ maxBufferSize: 6 }).makeUnsafe().decode('"ééé"\n')[0] !== "ééé") throw new Error("NDJSON limit must count UTF-16 units");
if (Serialization.makeNdjson().contentType !== "application/x-ndjson") throw new Error("NDJSON content type changed");
const Effect = await import("../../packages/effect-ts/node_modules/effect/dist/Effect.js");
const jsonrpc = Effect.runSync(Effect.provide(Effect.service(Serialization.RpcSerialization), Serialization.layerJsonRpc())).makeUnsafe();
const discarded = { _tag: "Request", id: "7", tag: "work", payload: { a: 1 }, headers: [], discard: true };
const discardedWire = '{"jsonrpc":"2.0","method":"work","params":{"a":1},"id":"7","discard":true}';
if (jsonrpc.encode(discarded) !== discardedWire || jsonrpc.decode(discardedWire)[0]?.discard !== true) throw new Error("JSON-RPC must preserve discard");
