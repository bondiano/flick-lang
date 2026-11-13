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
const { schemaInt, schemaNumber } = await import("../../packages/effect-ts/dist/schema/schemas.js");
for (const schema of [schemaInt, schemaNumber]) {
  try {
    Binary.encoder(Schema.toEncoded(tree(schema)));
  } catch (error) {
    if (error.message.includes("union members are not uniquely identifiable")) continue;
    throw error;
  }
  throw new Error("expected Flick numeric string-tree binary layout to be rejected");
}
