// Compare HashMap/HashSet Seq traversal and construction time and heap allocation.
import assert from "node:assert/strict";
import { Session } from "node:inspector/promises";
import { performance } from "node:perf_hooks";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = process.argv[2] ?? fileURLToPath(new URL("../../", import.meta.url));
const load = (path) => import(pathToFileURL(resolve(root, path)));

const rt = await load("runtime/dist/rt/collections.js");
const seq = await load("runtime/dist/core/seq.js");
const hashMod = await load("runtime/dist/rt/hash.js");
const effHash = await load("packages/effect-ts/dist/stdlib/hash.js");
const interval = 4096;

let consumed;
function run(operation, repeats) {
  for (let i = 0; i < repeats; i++) consumed = operation();
}

function sampledBytes(node) {
  return node.children.reduce((bytes, child) => bytes + sampledBytes(child), node.selfSize);
}

async function measure(operation, repeats) {
  run(operation, repeats);
  run(operation, repeats);
  const times = [];
  for (let sample = 0; sample < 7; sample++) {
    const start = performance.now();
    run(operation, repeats);
    times.push((performance.now() - start) / repeats);
  }
  times.sort((left, right) => left - right);

  const session = new Session();
  session.connect();
  let allocation;
  try {
    await session.post("HeapProfiler.startSampling", {
      samplingInterval: interval,
      includeObjectsCollectedByMajorGC: true,
      includeObjectsCollectedByMinorGC: true,
    });
    run(operation, repeats);
    const { profile } = await session.post("HeapProfiler.stopSampling");
    allocation = sampledBytes(profile.head) / repeats;
  } finally {
    session.disconnect();
  }
  return [times[3], allocation];
}

const witness = { eq: (a, b) => a === b, hash: hashMod.hash };

console.log(`# Node ${process.version} ${process.platform}/${process.arch}; V8 heap allocation estimate`);
console.log("operation,size,median_ms,estimated_allocated_bytes");

for (const size of [1024, 4096]) {
  const pairs = seq.seqMap((i) => [i, i * 2], seq.seqRange(0, size));
  const ints = seq.seqRange(0, size);

  const rtMap = rt.hashMapFromSeq(witness, pairs);
  const rtSet = rt.hashSetFromSeq(witness, ints);
  const effMap = effHash.hashMapFromSeq(witness, pairs);
  const effSet = effHash.hashSetFromSeq(witness, ints);

  assert.deepEqual(
    seq.seqToList(seq.seqTake(5, rt.hashMapToSeq(rtMap))),
    seq.seqToList(seq.seqTake(5, effHash.hashMapToSeq(effMap))),
  );
  assert.deepEqual(
    seq.seqToList(seq.seqTake(5, rt.hashSetToSeq(rtSet))),
    seq.seqToList(seq.seqTake(5, effHash.hashSetToSeq(effSet))),
  );

  for (const [name, operation, repeats] of [
    ["rt.map.prefix5.lazy", () => seq.seqToList(seq.seqTake(5, rt.hashMapToSeq(rtMap))), 1000],
    ["rt.map.prefix5.eager", () => seq.seqToList(seq.seqTake(5, seq.seqOfList(rt.hashMapToList(rtMap)))), 50],
    ["rt.set.prefix5.lazy", () => seq.seqToList(seq.seqTake(5, rt.hashSetToSeq(rtSet))), 1000],
    ["rt.set.prefix5.eager", () => seq.seqToList(seq.seqTake(5, seq.seqOfList(rt.hashSetToList(rtSet)))), 50],
    ["rt.map.from_seq", () => rt.hashMapFromSeq(witness, pairs), 15],
    ["rt.set.from_seq", () => rt.hashSetFromSeq(witness, ints), 15],
    ["eff.map.prefix5.lazy", () => seq.seqToList(seq.seqTake(5, effHash.hashMapToSeq(effMap))), 1000],
    ["eff.map.prefix5.eager", () => seq.seqToList(seq.seqTake(5, seq.seqOfList(effHash.hashMapToList(effMap)))), 50],
    ["eff.set.prefix5.lazy", () => seq.seqToList(seq.seqTake(5, effHash.hashSetToSeq(effSet))), 1000],
    ["eff.set.prefix5.eager", () => seq.seqToList(seq.seqTake(5, seq.seqOfList(effHash.hashSetToList(effSet)))), 50],
    ["eff.map.from_seq", () => effHash.hashMapFromSeq(witness, pairs), 15],
    ["eff.set.from_seq", () => effHash.hashSetFromSeq(witness, ints), 15],
  ]) {
    const [time, allocated] = await measure(operation, repeats);
    assert.notEqual(consumed, undefined);
    console.log(`${name},${size},${time.toFixed(4)},${Math.round(allocated)}`);
  }
}
