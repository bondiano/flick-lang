// A record without properties and of one index signature: keys drawn from
// its key's node, each new, each with a value of its value's node; shrunk
// by removing an entry, then by shrinking a value, then a key to one no
// other entry has.
import { item } from "../../internal.ts";
import {
  type Gen, type Pull, type Retained, type Sample, compiledGen, concatPulls, filterSample, fromRetained, lazyShrinks,
  mapPull, mapRetained, pullEach, replaceAt, reserving, retain, sampleOf,
} from "./model.ts";
import { randomLength, withSize } from "./random.ts";

/** The object of the entries; `__proto__` is a key like any other. */
const entriesRecord = (keys: ReadonlyArray<unknown>, values: ReadonlyArray<unknown>): Record<string, unknown> => {
  const out: Record<string, unknown> = {};
  for (const [i, key] of keys.entries())
    Object.defineProperty(out, String(key), { value: values[i], writable: true, enumerable: true, configurable: true });
  return out;
};

interface Entry { readonly key: unknown; readonly keySample: Retained; readonly value: Retained }

const entriesSample = (keys: ReadonlyArray<Sample>, values: ReadonlyArray<Sample>, shrinks: boolean): Sample => {
  const value = entriesRecord(keys.map((k) => k.value), values.map((v) => v.value));
  if (!shrinks) return sampleOf(value);
  const rebuild = (entries: ReadonlyArray<Entry>, valueStart: number, keyStart: number): Sample => {
    const pulls: Array<Pull> = [];
    if (entries.length > 0)
      pulls.push(pullEach([...entries.keys()], (i) => rebuild([...entries.slice(0, i), ...entries.slice(i + 1)], 0, 0)));
    for (let i = valueStart; i < entries.length; i++) {
      const e = item(entries, i);
      if (e.value.shrinks !== undefined)
        pulls.push(mapRetained(e.value.shrinks(), (r) => rebuild(replaceAt(entries, i, { ...e, value: r }), i, 0)));
    }
    for (let i = keyStart; i < entries.length; i++) {
      const e = item(entries, i);
      if (e.keySample.shrinks === undefined) continue;
      const filtered = filterSample(fromRetained(e.keySample),
        (k) => !entries.some((other, j) => j !== i && other.key === k));
      if (filtered?.shrinks === undefined) continue;
      pulls.push(mapPull(filtered.shrinks,
        (s) => rebuild(replaceAt(entries, i, { ...e, key: s.value, keySample: retain(s) }), entries.length, i)));
    }
    return sampleOf(entriesRecord(entries.map((e) => e.key), entries.map((e) => e.value.value)),
      pulls.length > 0 ? concatPulls(pulls) : undefined);
  };
  return lazyShrinks(value, () => rebuild(keys.map((k, i) =>
    ({ key: k.value, keySample: retain(k), value: retain(item(values, i)) })), 0, 0).shrinks);
};

export const compileRecord = (value: Gen, key: Gen): Gen => compiledGen([key, value], () => 0, (st) => {
  const upper = Math.max(0, st.size);
  const indexCost = key.minCost + value.minCost;
  const affordable = indexCost === Infinity ? 0 : indexCost === 0 ? upper : Math.min(upper, Math.floor(st.budget.remaining / indexCost));
  if (affordable < 0) return null;
  const count = randomLength(st, 0, affordable);
  if ((count > 0 && count * indexCost > st.budget.remaining) || (count === 0 && st.budget.remaining < 0)) return null;
  const keys: Array<Sample> = [];
  const values: Array<Sample> = [];
  for (let position = 0; position < count; position++) {
    const future = (count - position - 1) * indexCost;
    if (indexCost + future > st.budget.remaining) return null;
    const budget = st.budget.remaining;
    const keyState = st.size < count ? withSize(st, count) : st;
    let k: Sample | null;
    for (let retries = 0; ; retries++) {
      k = reserving(key, keyState, value.minCost + future);
      if (k === null) return null;
      const drawn = k.value;
      if (!keys.some((other) => other.value === drawn)) break;
      if (retries >= 10) return null;
      st.budget.remaining = budget;
    }
    const v = reserving(value, st, future);
    if (v === null) return null;
    keys.push(k);
    values.push(v);
  }
  return entriesSample(keys, values, st.shrinks);
});
