// ReadonlyRecord values are plain objects. Preserve their own enumerable key
// order, including integer keys as JavaScript orders them.
export const toEntries = <A>(record: Readonly<Record<string, A>>): Array<[string, A]> => Object.entries(record);
