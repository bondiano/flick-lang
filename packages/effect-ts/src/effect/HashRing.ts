// Effect's `HashRing` for the bindings of the standard library. Effect's ring
// changes in place and needs nodes with a `PrimaryKey`; Flick's is persistent:
// each change works on a copy, and each node is boxed with the key the ring's
// key function gives it.
import { HashRing, Option, PrimaryKey } from "effect";

export * from "effect/HashRing";

class Node<A> implements PrimaryKey.PrimaryKey {
  readonly value: A;
  readonly key: string;
  constructor(value: A, key: string) {
    this.value = value;
    this.key = key;
    Object.freeze(this);
  }
  [PrimaryKey.symbol](): string {
    return this.key;
  }
}

/** `HashRing.HashRing a`: Effect's ring of boxed nodes and the key function. */
export interface FlickHashRing<A> {
  readonly key: (a: A) => string;
  readonly native: HashRing.HashRing<Node<A>>;
}

const ring = <A>(key: (a: A) => string, native: HashRing.HashRing<Node<A>>): FlickHashRing<A> =>
  Object.freeze({ key, native });

const node = <A>(self: FlickHashRing<A>, a: A): Node<A> => new Node(a, self.key(a));

const copy = <A>(self: FlickHashRing<A>): HashRing.HashRing<Node<A>> => {
  const next = HashRing.make<Node<A>>({ baseWeight: self.native.baseWeight });
  for (const [key, [n, weight]] of self.native.nodes) next.nodes.set(key, [n, weight]);
  next.ring = self.native.ring.slice();
  next.totalWeightCache = self.native.totalWeightCache;
  return next;
};

/** `HashRing.make options key`. */
export const flickMake = <A>(options: { readonly baseWeight: Option.Option<number> }, key: (a: A) => string): FlickHashRing<A> =>
  ring(key, HashRing.make({ baseWeight: Option.getOrUndefined(options.baseWeight) }));

/** `HashRing.add_many options nodes ring`. */
export const flickAddMany = <A>(
  options: { readonly weight: Option.Option<number> },
  nodes: ReadonlyArray<A>,
  self: FlickHashRing<A>,
): FlickHashRing<A> =>
  ring(
    self.key,
    HashRing.addMany(
      copy(self),
      nodes.map((a) => node(self, a)),
      { weight: Option.getOrUndefined(options.weight) },
    ),
  );

/** `HashRing.add options node ring`. */
export const flickAdd = <A>(options: { readonly weight: Option.Option<number> }, a: A, self: FlickHashRing<A>): FlickHashRing<A> =>
  ring(self.key, HashRing.add(copy(self), node(self, a), { weight: Option.getOrUndefined(options.weight) }));

export const flickRemove = <A>(a: A, self: FlickHashRing<A>): FlickHashRing<A> =>
  ring(self.key, HashRing.remove(copy(self), node(self, a)));

export const flickHas = <A>(a: A, self: FlickHashRing<A>): boolean => HashRing.has(self.native, node(self, a));

export const flickGet = <A>(input: string, self: FlickHashRing<A>): Option.Option<A> => {
  const found = HashRing.get(self.native, input);
  return found === undefined ? Option.none() : Option.some(found.value);
};

export const flickGetShards = <A>(count: number, self: FlickHashRing<A>): Option.Option<Array<A>> => {
  const shards = HashRing.getShards(self.native, count);
  return shards === undefined ? Option.none() : Option.some(shards.map((n) => n.value));
};
