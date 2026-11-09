export const toJson = (value: unknown): unknown => value;
export const flickToStringUnknown = (indent: number, value: unknown): string =>
  JSON.stringify(value, null, indent > 0 ? indent : undefined) ?? String(value);
