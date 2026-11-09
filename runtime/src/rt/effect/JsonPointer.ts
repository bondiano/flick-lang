export const escapeToken = (token: string): string => token.replaceAll("~", "~0").replaceAll("/", "~1");
export const unescapeToken = (token: string): string => token.replaceAll("~1", "/").replaceAll("~0", "~");
export const parseUriFragment = (fragment: string): Array<string> | null => {
  if (fragment === "#") return [];
  if (!fragment.startsWith("#/")) return null;
  try {
    return fragment.slice(2).split("/").map((token) => unescapeToken(decodeURIComponent(token)));
  } catch { return null; }
};
export const formatUriFragment = (tokens: ReadonlyArray<string>): string =>
  `#${tokens.map((token) => `/${encodeURIComponent(escapeToken(token))}`).join("")}`;
