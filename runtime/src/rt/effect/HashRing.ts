// Persistent weighted hash ring; a node is known by a user supplied string key.
// Point ordering and shard allocation are derived from effect@4.1.2 HashRing (MIT).
import { none, some, type Option } from "../../core/data.ts";
import { item } from "../../internal.ts";
import { string as hashString } from "./Hash.ts";

interface Ring<A> {
  readonly key: (value: A) => string;
  readonly baseWeight: number;
  readonly nodes: ReadonlyMap<string, readonly [A, number]>;
  readonly points: ReadonlyArray<readonly [number, string]>;
  readonly totalWeight: number;
}
const value = <A>(option: Option<A>, fallback: A): A => option._tag === "Some" ? option.value : fallback;
const entry = <K, V>(map: ReadonlyMap<K, V>, key: K): V => {
  const found = map.get(key);
  if (found === undefined) throw new Error("missing ring node");
  return found;
};
const makeRing = <A>(key: (value: A) => string, baseWeight: number, nodes: ReadonlyMap<string, readonly [A, number]>): Ring<A> => {
  const points: Array<readonly [number, string]> = [];
  for (const [name, [, weight]] of nodes) {
    for (let index = Math.round(weight * baseWeight); index > 0; index--) points.push([hashString(`${name}:${index}`), name]);
  }
  const ordered = points.toSorted((a, b) => a[0] - b[0] || (a[1] < b[1] ? -1 : a[1] > b[1] ? 1 : 0));
  let totalWeight = 0;
  for (const name of [...nodes.keys()].toSorted()) totalWeight += nodes.get(name)?.[1] ?? 0;
  return { key, baseWeight, nodes, points: ordered, totalWeight };
};
export const flickMake = <A>(options: { readonly baseWeight: Option<number> }, key: (value: A) => string): Ring<A> =>
  makeRing(key, Math.max(value(options.baseWeight, 128), 1), new Map());
export const flickAddMany = <A>(options: { readonly weight: Option<number> }, values: ReadonlyArray<A>, ring: Ring<A>): Ring<A> => {
  const weight = Math.max(value(options.weight, 1), 0.1);
  const nodes = new Map(ring.nodes);
  for (const node of values) nodes.set(ring.key(node), [node, weight]);
  return makeRing(ring.key, ring.baseWeight, nodes);
};
export const flickAdd = <A>(options: { readonly weight: Option<number> }, node: A, ring: Ring<A>): Ring<A> =>
  flickAddMany(options, [node], ring);
export const flickRemove = <A>(node: A, ring: Ring<A>): Ring<A> => {
  const nodes = new Map(ring.nodes);
  nodes.delete(ring.key(node));
  return makeRing(ring.key, ring.baseWeight, nodes);
};
export const flickHas = <A>(node: A, ring: Ring<A>): boolean => ring.nodes.has(ring.key(node));

const nearest = <A>(ring: Ring<A>, hash: number, exclude?: ReadonlySet<string>): readonly [number, number] => {
  const points = ring.points;
  const length = points.length;
  let lo = 0;
  let hi = length - 1;
  while (lo <= hi) {
    const mid = ((lo + hi) / 2) >>> 0;
    if (item(points, mid)[0] >= hash) hi = mid - 1;
    else lo = mid + 1;
  }
  const first = lo === length ? lo - 1 : lo;
  const distance = Math.abs(item(points, first)[0] - hash);
  if (exclude === undefined) {
    const previous = lo - 1;
    if (previous < 0) return [first, distance];
    const previousDistance = Math.abs(item(points, previous)[0] - hash);
    return distance <= previousDistance ? [first, distance] : [previous, previousDistance];
  }
  if (!exclude.has(item(points, first)[1])) return [first, distance];
  const range = Math.max(lo, length - lo);
  for (let step = 1; step <= range; step++) {
    const left = lo - step;
    if (left >= 0 && left < length && !exclude.has(item(points, left)[1])) return [left, Math.abs(item(points, left)[0] - hash)];
    const right = lo + step;
    if (right >= 0 && right < length && !exclude.has(item(points, right)[1])) return [right, Math.abs(item(points, right)[0] - hash)];
  }
  return [first, distance];
};
const at = <A>(ring: Ring<A>, index: number): readonly [A, number] => entry(ring.nodes, item(ring.points, index)[1]);
export const flickGet = <A>(input: string, ring: Ring<A>): Option<A> =>
  ring.points.length === 0 ? none : some(at(ring, nearest(ring, hashString(input))[0])[0]);
export const flickGetShards = <A>(requested: number, ring: Ring<A>): Option<Array<A>> => {
  if (ring.points.length === 0) return none;
  const count = Number.isFinite(requested) && requested > 0 ? Math.floor(requested) : 0;
  const shards = Array.from<A>({ length: count });
  const allocations = new Map<string, number>();
  const remaining = new Set<number>();
  const exclude = new Set<string>();
  const distances: Array<readonly [number, string, number]> = [];
  const hashes: Array<number> = [];
  for (let shard = 0; shard < count; shard++) {
    const hash = hashString(`shard-${shard}`);
    hashes.push(hash);
    const [index, distance] = nearest(ring, hash);
    distances.push([shard, item(ring.points, index)[1], distance]);
    remaining.add(shard);
  }
  const ordered = distances.toSorted((a, b) => a[2] - b[2]);
  const allocate = (shard: number, name: string): void => {
    const [node, weight] = entry(ring.nodes, name);
    shards[shard] = node;
    const nodeCount = (allocations.get(name) ?? 0) + 1;
    allocations.set(name, nodeCount);
    const maximum = Math.max(1, Math.floor(count * weight / ring.totalWeight));
    if (nodeCount >= maximum) exclude.add(name);
  };
  for (const [shard, name] of ordered) {
    if (exclude.has(name)) continue;
    allocate(shard, name);
    remaining.delete(shard);
  }
  for (const shard of remaining) {
    const allAtMax = exclude.size === ring.nodes.size;
    const index = nearest(ring, item(hashes, shard), allAtMax ? undefined : exclude)[0];
    allocate(shard, item(ring.points, index)[1]);
  }
  return some(shards);
};
