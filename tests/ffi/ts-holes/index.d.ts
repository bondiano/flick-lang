// The TypeScript holes of docs/formal/ts-holes.md as the importer meets them.
export { type Options as RetryOptions, retry } from "./a.js";
export { type Options as LogOptions, logOptions } from "./b.js";

/** Members the importer never reads: a brand, a getter, a setter without a getter, an index signature. */
export declare class Point {
  protected readonly brand: "Point";
  readonly x: number;
  get length(): number;
  set label(value: string);
  [key: string]: unknown;
  constructor(x: number);
}

export declare function makePoint(x: number): Point;
export declare function register(ctor: typeof Point): void;
export declare function isPoint(value: unknown): value is Point;
export declare function first(xs: readonly number[]): number;
export declare function scores(): Record<string, number>;
