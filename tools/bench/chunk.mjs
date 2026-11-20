// Compare runtime allocation and time on the same host.
// Pass the root of a baseline checkout to run the same workloads there.
import assert from "node:assert/strict";
import { Session } from "node:inspector/promises";
import { performance } from "node:perf_hooks";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = process.argv[2] ?? fileURLToPath(new URL("../../", import.meta.url));
const runtime = (path) => import(pathToFileURL(resolve(root, "runtime/src/rt", path)));
const chunk = await runtime("chunk.ts");
const { reduce } = await runtime("effect/Chunk.ts");
const interval = 8192;

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

  // Profile separately so the sampling overhead is outside the timing.
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

console.log(`# Node ${process.version} ${process.platform}/${process.arch}; V8 heap allocation estimate`);
console.log("operation,size,median_ms,estimated_allocated_bytes");
for (const size of [1024, 8192]) {
  const values = Array.from({ length: size }, (_, i) => i);
  const batches = [];
  for (let start = 0; start < size; start += 16) {
    batches.push(chunk.chunkFromList(values.slice(start, start + 16)));
  }

  const append = () => {
    let out = chunk.chunkEmpty;
    for (const value of values) out = chunk.chunkAppend(value, out);
    return out;
  };
  const concat = () => {
    let out = chunk.chunkEmpty;
    for (const batch of batches) out = chunk.chunkConcat(out, batch);
    return out;
  };
  const assembled = concat();
  const lookup = () => {
    let sum = 0;
    for (let i = 0; i < size; i++) sum += chunk.chunkGet((i * 131) % size, assembled).value;
    return sum;
  };
  const fold = reduce(0, (sum, value) => sum + value);

  const sum = (size * (size - 1)) / 2;
  assert.deepEqual(chunk.chunkToList(append()), values);
  assert.deepEqual(chunk.chunkToList(assembled), values);
  assert.equal(lookup(), sum);
  assert.equal(fold(assembled), sum);

  const buildRepeats = size === 1024 ? 12 : 3;
  for (const [name, operation, repeats] of [
    ["append", append, buildRepeats],
    ["concat-16", concat, buildRepeats],
    ["lookup", lookup, 64],
    ["traversal", () => fold(assembled), 64],
    ["to-list", () => chunk.chunkToList(assembled), 64],
  ]) {
    const [time, allocated] = await measure(operation, repeats);
    assert.notEqual(consumed, undefined);
    console.log(`${name},${size},${time.toFixed(4)},${Math.round(allocated)}`);
  }
}
