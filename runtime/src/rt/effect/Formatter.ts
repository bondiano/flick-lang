export const format = (value: unknown): string => JSON.stringify(value) ?? String(value);
export const flickFormatJson = (indent: number, value: unknown): string =>
  JSON.stringify(value, null, indent > 0 ? indent : undefined) ?? String(value);
