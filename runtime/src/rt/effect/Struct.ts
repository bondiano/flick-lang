// `Struct.Record`: one value under each key in its input list.
export const Record = <A>(keys: ReadonlyArray<string>, value: A): Readonly<Record<string, A>> =>
  Object.fromEntries(keys.map((key) => [key, value]));
interface Combiner<A> { readonly combine: (a: A, b: A) => A }
interface Reducer<A> extends Combiner<A> { readonly initial: A }
type Fields = Readonly<Record<string, unknown>>;
type Combiners = Readonly<Record<string, Combiner<unknown>>>;
type Reducers = Readonly<Record<string, Reducer<unknown>>>;
const field = <A>(fields: Readonly<Record<string, A>>, key: string): A => {
  if (!Object.hasOwn(fields, key)) throw new Error("missing struct field");
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- hasOwn checked above
  return fields[key] as A;
};
export const flickMakeCombiner = (_map: unknown, fields: Combiners): Combiner<Fields> => {
  const keys = Object.keys(fields).toSorted();
  return {
    combine: (a, b) => {
      const out: Record<string, unknown> = {};
      for (const key of keys) {
        Object.defineProperty(out, key, {
          value: field(fields, key).combine(field(a, key), field(b, key)),
          enumerable: true,
          writable: true,
          configurable: true,
        });
      }
      return out;
    },
  };
};
export const flickMakeReducer = (_map: unknown, fields: Reducers): Reducer<Fields> => {
  const keys = Object.keys(fields).toSorted();
  const initial: Record<string, unknown> = {};
  for (const key of keys) {
    Object.defineProperty(initial, key, {
      value: field(fields, key).initial,
      enumerable: true,
      writable: true,
      configurable: true,
    });
  }
  return {
    initial,
    combine: flickMakeCombiner(_map, fields).combine,
  };
};
