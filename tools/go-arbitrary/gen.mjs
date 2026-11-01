// Writes runtime-go/flickrt/testdata/arbitrary_links.txt: what Effect's
// Arbitrary generates, checks and shrinks on Node for the schemas whose
// declarations it generates through a link (Unknown, Duration, Chunk,
// DateTime and its zones) or cannot generate (HashSet, HashMap), as the JS
// runtime builds them. TestArbitraryLinksAsEffect compares the go
// runtime's lines with these.
// Run from the repository root after `pnpm -C runtime build`:
// `node tools/go-arbitrary/gen.mjs`.
import { writeFileSync } from "node:fs";

import * as rt from "../../runtime/dist/index.js";
import * as S from "../../runtime/dist/effect/Schema.js";
import * as D from "../../runtime/dist/effect/DateTime.js";
import { Arbitrary, Chunk, DateTime, Duration, Effect, Exit } from "../../runtime/node_modules/effect/dist/index.js";

const out = "runtime-go/flickrt/testdata/arbitrary_links.txt";

// a string as the Go test writes it: its UTF-16 units, printable ASCII as is
const quote = (s) => {
  let b = '"';
  for (let i = 0; i < s.length; i++) {
    const u = s.charCodeAt(i);
    if (u === 0x22) b += '\\"';
    else if (u === 0x5c) b += "\\\\";
    else if (u >= 0x20 && u <= 0x7e) b += s[i];
    else b += `\\u${u.toString(16).padStart(4, "0")}`;
  }
  return `${b}"`;
};

const zone = (z) => (z._tag === "Offset" ? `offset(${z.offset})` : z.id);

const show = (v) => {
  if (v === null) return "null";
  if (v === undefined) return "()";
  if (typeof v === "number") return v === 0 ? "0" : String(v);
  if (typeof v === "string") return quote(v);
  if (typeof v === "boolean") return String(v);
  if (typeof v === "bigint") return `${v}n`;
  if (Array.isArray(v)) return `[${v.map(show).join(",")}]`;
  if (Duration.isDuration(v)) return `dur(${String(v)})`;
  if (Chunk.isChunk(v)) return `chunk${show(Chunk.toReadonlyArray(v))}`;
  if (DateTime.isDateTime(v)) {
    return DateTime.isUtc(v) ? `utc(${show(v.epochMilliseconds)})` : `zoned(${show(v.epochMilliseconds)},${zone(v.zone)})`;
  }
  if (DateTime.isTimeZone(v)) return zone(v);
  const keys = Object.keys(v);
  return `{${keys.map((k) => `${/^[A-Za-z_][A-Za-z0-9_]*$/.test(k) ? k : quote(k)}:${show(v[k])}`).join(",")}}`;
};

const isRecord = (v) => typeof v === "object" && v !== null && !Array.isArray(v);
const hashInt = { hash: (n) => n, $Eq: { eq: (a) => (b) => a === b } };

const cases = [
  ["unknown", () => rt.schemaUnknown, (v) => !Array.isArray(v)],
  ["unknown-record", () => rt.schemaUnknown, (v) => !(isRecord(v) && Object.keys(v).length > 0)],
  ["unknown-fields", () => rt.schemaRecord([["a", rt.schemaUnknown], ["b", rt.schemaUnknown]]), (v) => !Array.isArray(v.a)],
  ["opaque", () => rt.schemaOpaque("Conn", "none"), (v) => typeof v !== "string"],
  ["redacted-json", () => S.flickToType(rt.schemaJsonCodec(rt.schemaRedacted(rt.schemaBool))), (v) => v !== null],
  ["duration", () => S.flickDuration, (v) => Duration.toMillis(v) < 1000],
  ["duration-millis", () => S.flickToType(S.flickDurationFromMillis), (v) => v.value._tag !== "Nanos"],
  ["duration-string", () => S.flickToType(S.flickDurationFromString), (v) => v.value._tag !== "Millis"],
  ["chunk", () => S.flickChunk(rt.schemaBool), (v) => Chunk.size(v) < 3],
  ["chunk-json", () => S.flickToType(S.flickChunkJson(rt.schemaJsonCodec(rt.schemaString))), (v) => Chunk.size(v) < 2],
  ["utc", () => S.flickToType(D.flickJsonUtc), (v) => v.epochMilliseconds < 0],
  ["utc-millis", () => S.flickToType(D.flickJsonUtcMillis), (v) => v.epochMilliseconds > 0],
  ["zoned", () => S.flickToType(D.flickJsonZoned), (v) => v.zone._tag === "Offset"],
  ["zone", () => S.flickToType(D.flickJsonZone), (v) => v._tag === "Offset"],
  ["zone-named", () => S.flickToType(D.flickJsonZoneNamed), (v) => v.id !== "Asia/Tokyo"],
  ["zone-offset", () => S.flickToType(D.flickJsonZoneOffset), (v) => v.offset < 1000],
  ["hashset", () => S.flickToType(S.flickHashSetJson(hashInt, rt.codecInt)), () => true],
  ["hashmap", () => S.flickToType(S.flickHashMapJson(hashInt, rt.codecInt, rt.codecInt)), () => true],
  ["result-json", () => S.flickToType(rt.schemaJsonCodec(rt.schemaResult(rt.schemaString, rt.schemaBool))), () => true],
];

const lines = [];
for (const [name, schema, property] of cases) {
  for (const seed of [1, 42]) {
    const prefix = `${name} seed ${seed} `;
    let arb;
    try {
      arb = Arbitrary.schema(schema());
    } catch (e) {
      lines.push(`${prefix}error ${String(e)}`);
      continue;
    }
    const sampled = Effect.runSyncExit(Arbitrary.sampleEffect(arb, { count: 8, seed }));
    if (Exit.isSuccess(sampled)) lines.push(`${prefix}sample ${show(sampled.value)}`);
    else {
      const e = sampled.cause.reasons[0].error;
      lines.push(`${prefix}sample error ${e.generated} ${e.discards}`);
    }
    const check = (options) =>
      Effect.runSync(Arbitrary.checkEffect(arb, (v) => Effect.succeed(property(v)), { runs: 30, ...options }));
    const r = check({ seed });
    switch (r._tag) {
      case "Passed":
        lines.push(`${prefix}check passed ${r.runs} ${r.discards}`);
        break;
      case "Falsified": {
        lines.push(
          `${prefix}check falsified ${r.runs} ${r.discards} ${r.shrinks} ${show(r.initialInput)} ${show(r.shrunkInput)} ${r.replay}`,
        );
        const again = check({ replay: r.replay });
        lines.push(
          again._tag === "Falsified"
            ? `${prefix}replay Falsified ${show(again.shrunkInput)}`
            : `${prefix}replay ReplayMismatch ${again.reason}`,
        );
        break;
      }
      case "Exhausted":
        lines.push(`${prefix}check exhausted ${r.runs} ${r.discards} ${String(r.seed)}`);
        break;
    }
  }
}
writeFileSync(out, `${lines.join("\n")}\n`);
