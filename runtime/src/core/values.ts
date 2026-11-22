// The data ABI types, constructors, runtime protocol symbols, and foreign
// boundary copying of compiled Flick code without Effect.
import { tag } from "../tags.ts";

export type Option<A> = Readonly<{ _tag: "None" }> | Readonly<{ _tag: "Some"; value: A }>;
export type Result<A, E = never> = Readonly<{ _tag: "Success"; success: A }> | Readonly<{ _tag: "Failure"; failure: E }>;

/** A value of a variant type: its qualified constructor id and positional fields. */
export interface Data {
  readonly _tag: string;
  readonly [field: `_${number}`]: unknown;
}

/** A value of a variant type with one field `_0`. */
export interface Tagged<T extends string, A> {
  readonly _tag: T;
  readonly _0: A;
}

/** `Prelude.Ordering`. */
export type Ordering = Readonly<{ _tag: string }>;

/** Dictionaries of the built-in classes, curried as generated code calls them. */
export interface EqDict<A> {
  readonly eq: (a: A) => (b: A) => boolean;
}
export interface OrdDict<A> {
  readonly compare: (a: A) => (b: A) => Ordering;
  readonly $Eq: EqDict<A>;
}
export interface HashDict<A> {
  readonly hash: (a: A) => number;
  readonly $Eq: EqDict<A>;
}
export interface ShowDict<A> {
  readonly show: (a: A) => string;
}

export const some = <A>(value: A): Option<A> => ({ _tag: "Some", value });
export const none: Option<never> = /*#__PURE__*/ Object.freeze({ _tag: "None" });
export const ok = <A>(success: A): Result<A> => ({ _tag: "Success", success });
export const err = <E>(failure: E): Result<never, E> => ({ _tag: "Failure", failure });

const ordering = (name: "LT" | "EQ" | "GT"): Ordering => Object.freeze({ _tag: tag(`Prelude.Ordering.${name}`) });
export const LT = /*#__PURE__*/ ordering("LT");
export const EQ = /*#__PURE__*/ ordering("EQ");
export const GT = /*#__PURE__*/ ordering("GT");

/** A native runtime value whose equality is defined by its semantic value. */
export const runtimeEquals = /*#__PURE__*/ Symbol.for("@flick-lang/runtime/equals");
export const runtimeHash = /*#__PURE__*/ Symbol.for("@flick-lang/runtime/hash");
/** Prototype marker for GADT values holding `$0`, `$1`, … dictionaries. */
export const gadtMarker = /*#__PURE__*/ Symbol.for("@flick-lang/runtime/gadt");
/** A native sequence exposed through iteration instead of array indices. */
export const runtimeSequence = /*#__PURE__*/ Symbol();
export const isRuntimeSequence = (value: object, carrier = value): value is Iterable<unknown> =>
  runtimeSequence in carrier;

/** `nullable`: `null` and `undefined` are `None`. */
export const fromNullable = <A>(value: A | null | undefined): Option<A> =>
  value === null || value === undefined ? none : some(value);

/** `optional i`: `None` is passed as `undefined`. */
export const optionalArg = <A>(option: Option<A>): A | undefined => (option._tag === "Some" ? option.value : undefined);

const putOwn = (target: object, key: PropertyKey, value: unknown): void => {
  Object.defineProperty(target, key, { value, writable: true, enumerable: true, configurable: true });
};

/**
 * `value` as Flick data JS can no longer change: arrays and plain objects
 * are copied all the way down, and any other object goes through `other`
 * (`docs/formal/ts-holes.md#31-flick-data-is-copied-at-the-foreign-boundary`). A frozen object is a constant such as
 * `none`, kept with its identity.
 * Traversal uses a worklist; cyclic mutable data still fails with RangeError,
 * not a mutable cyclic Flick value. Arrays become fresh plain arrays: foreign
 * map overrides and species constructors cannot bypass isolation. Own record
 * fields are defined as data fields, including `__proto__`.
 * shortcut: a plain JS object typed `JsValue` inside data is copied too; type the copy if a binding needs its identity.
 */
export const copyThrough = (value: unknown, other: (value: object) => unknown): unknown => {
  const work: Array<() => void> = [];
  const active = new WeakSet<object>();
  let result: unknown;
  const visit = (input: unknown, put: (copy: unknown) => void): void => {
    if (typeof input !== "object" || input === null) { put(input); return; }
    let array: unknown = input;
    let preserveProto: object | null = null;
    if (!Array.isArray(input)) {
      const proto = Reflect.getPrototypeOf(input);
      if (proto !== Object.prototype && proto !== null) {
        if (gadtMarker in input) {
          preserveProto = proto;
        } else {
          if (!isRuntimeSequence(input, proto)) { put(other(input)); return; }
          array = Array.from(input);
        }
      } else if (Object.isFrozen(input)) { put(other(input)); return; }
    }
    // Only active ancestors are rejected. Completed aliases must be copied again,
    // not memoized: foreign mutation of one copy must not change another copy.
    if (active.has(input)) throw new RangeError("Maximum call stack size exceeded");
    active.add(input);
    const finish = (copy: unknown): void => { active.delete(input); put(copy); };
    if (Array.isArray(array)) {
      const length = array.length;
      const copy: unknown[] = [];
      copy.length = length;
      let index = 0;
      const next = (): void => {
        while (index < length) {
          const key = index++;
          // Like Array.map, read an index only after the previous child finishes.
          if (!(key in array)) continue;
          const child: unknown = array[key];
          work.push(next);
          visit(child, (copied) => putOwn(copy, key, copied));
          return;
        }
        finish(copy);
      };
      work.push(next);
    } else {
      // Object.entries eagerly reads all enumerable fields before descending.
      const entries: Array<[string, unknown]> = Object.entries(input);
      const copy: Record<string, unknown> = {};
      if (preserveProto !== null) Object.setPrototypeOf(copy, preserveProto);
      let index = 0;
      const next = (): void => {
        const entry = entries[index++];
        if (entry === undefined) { finish(copy); return; }
        const [key, child] = entry;
        work.push(next);
        visit(child, (copied) => putOwn(copy, key, copied));
      };
      work.push(next);
    }
  };
  visit(value, (copy) => { result = copy; });
  while (work.length > 0) work.pop()?.();
  return result;
};

/** Flick data that crosses the boundary of an import outside the runtime. */
export const copyData = <A>(value: A): A =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a copy has the shape of what it copies
  copyThrough(value, (x) => x) as A;
