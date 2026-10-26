// D14: what fusion saves. The same pipeline as the compiler emits it in
// debug (a generator, then a generator per combinator, then a list) and in
// release (one loop pushing into the list):
//
//   Seq.to_list (Seq.map (fun x -> x * 10) (Seq.filter (fun x -> x > 1) (for { x <- xs } yield x + 1)))
//
// Run after `pnpm -C runtime build`: `node tools/fusion-bench/bench.mjs`.
import { seq, seqFilter, seqMap, seqToList } from "../../runtime/dist/core/seq.js";

const xs = Array.from({ length: 1000 }, (_, i) => i);

const chained = (xs) =>
  seqToList(
    seqMap(
      (x) => x * 10,
      seqFilter(
        (x) => x > 1,
        seq(function* () {
          for (const x of xs) yield x + 1;
        }),
      ),
    ),
  );

const fused = (xs) => {
  const out = [];
  for (const x of xs) {
    const y = x + 1;
    if (y > 1) out.push(y * 10);
  }
  return out;
};

const time = (label, run, rounds) => {
  for (let i = 0; i < rounds / 10; i++) run(xs);
  const start = performance.now();
  for (let i = 0; i < rounds; i++) run(xs);
  const per = (performance.now() - start) / rounds;
  console.log(`${label.padEnd(10)} ${(per * 1000).toFixed(2).padStart(9)} µs per 1000 elements`);
  return per;
};

if (JSON.stringify(chained(xs)) !== JSON.stringify(fused(xs))) throw new Error("the two pipelines differ");
const before = time("chained", chained, 5000);
const after = time("fused", fused, 5000);
console.log(`fused is ${(before / after).toFixed(1)}× faster`);
