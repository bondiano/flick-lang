import { equals, hash, iterableHash, runtimeEquals, runtimeHash, runtimeSequence, sameValue } from "./data.ts";

// Small chunks and bulk constructions use flat leaves. Concatenations share
// their operands in an AVL tree, weighted by element count for indexing.
export type Chunk<A> = ReadonlyArray<A> | ChunkBranch<A>;

export class ChunkBranch<A> implements Iterable<A> {
  readonly length: number;
  readonly height: number;
  readonly leftLength: number;
  readonly left: Chunk<A>;
  readonly right: Chunk<A>;

  constructor(left: Chunk<A>, right: Chunk<A>) {
    this.left = left;
    this.right = right;
    this.leftLength = left.length;
    this.length = this.leftLength + right.length;
    if (!Number.isSafeInteger(this.length)) throw new RangeError("Chunk size exceeds the safe integer range");
    this.height = 1 + Math.max(height(left), height(right));
  }

  // Keep the flat JSON form when a chunk is inspected without a schema.
  toJSON(): Array<A> {
    return Array.from(this);
  }

  get [runtimeSequence](): true {
    return true;
  }

  [runtimeEquals](other: unknown, strictZero = false): boolean {
    if (!(other instanceof ChunkBranch || Array.isArray(other)) || this.length !== other.length) return false;
    const right = other[Symbol.iterator]();
    const eq = strictZero ? sameValue : equals;
    for (const value of this) if (!eq(value, right.next().value)) return false;
    return true;
  }

  [runtimeHash](): number {
    return iterableHash(hash, this);
  }

  *[Symbol.iterator](): Iterator<A, undefined> {
    const pending: Array<Chunk<A>> = [this];
    for (let next = pending.pop(); next !== undefined; next = pending.pop()) {
      if (next instanceof ChunkBranch) pending.push(next.right, next.left);
      else yield* next;
    }
    return undefined;
  }
}

const height = (chunk: Chunk<unknown>): number => chunk instanceof ChunkBranch ? chunk.height : 0;
const leafLimit = 32;

// Joining along the taller spine can increase its height by at most one.
// Rotate on the way back up whenever that makes a node unbalanced.
const balance = <A>(left: Chunk<A>, right: Chunk<A>): ChunkBranch<A> => {
  if (left instanceof ChunkBranch && left.height > height(right) + 1) {
    const middle = left.right;
    if (middle instanceof ChunkBranch && middle.height > height(left.left)) {
      return new ChunkBranch(new ChunkBranch(left.left, middle.left), new ChunkBranch(middle.right, right));
    }
    return new ChunkBranch(left.left, new ChunkBranch(middle, right));
  }
  if (right instanceof ChunkBranch && right.height > height(left) + 1) {
    const middle = right.left;
    if (middle instanceof ChunkBranch && middle.height > height(right.right)) {
      return new ChunkBranch(new ChunkBranch(left, middle.left), new ChunkBranch(middle.right, right.right));
    }
    return new ChunkBranch(new ChunkBranch(left, middle), right.right);
  }
  return new ChunkBranch(left, right);
};

export const concatChunks = <A>(left: Chunk<A>, right: Chunk<A>): Chunk<A> => {
  if (left.length === 0) return right;
  if (right.length === 0) return left;
  if (left instanceof ChunkBranch && left.height > height(right) + 1) {
    return balance(left.left, concatChunks(left.right, right));
  }
  if (right instanceof ChunkBranch && right.height > height(left) + 1) {
    return balance(concatChunks(left, right.left), right.right);
  }
  if (!(left instanceof ChunkBranch) && !(right instanceof ChunkBranch) && left.length + right.length <= leafLimit) {
    return [...left, ...right];
  }
  return new ChunkBranch(left, right);
};
