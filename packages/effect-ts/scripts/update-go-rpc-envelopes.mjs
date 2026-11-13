// Refresh transport envelopes against the pinned Effect implementation.
// Schema-specific scalar vectors remain independent legacy vectors.
import { readFileSync, writeFileSync } from "node:fs";
import * as Binary from "effect/encoding/SchemaBinary";
import * as Message from "effect/rpc/RpcMessage";
const file = new URL("../../../runtime-go/flickrt/rpc_binary_vectors_test.go", import.meta.url);
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
