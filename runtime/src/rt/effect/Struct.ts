// `Struct.Record`: one value under each key in its input list.
export const Record = <A>(keys: ReadonlyArray<string>, value: A): Readonly<Record<string, A>> =>
  Object.fromEntries(keys.map((key) => [key, value]));
