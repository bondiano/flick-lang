// The `_tag` of a constructor, as the compiler writes it: the name and the
// 48 high bits of the FNV-1a 64 hash of the qualified id, so the tag keeps
// the identity of package, module, type and constructor.
const OFFSET = 0xcbf2_9ce4_8422_2325n;
const PRIME = 0x0100_0000_01b3n;
const MASK = (1n << 64n) - 1n;

/** `tag("Prelude.Ordering.LT")` is `"LT#…"`. */
export const tag = (id: string): string => {
  let hash = OFFSET;
  for (const byte of new TextEncoder().encode(id)) {
    hash = ((hash ^ BigInt(byte)) * PRIME) & MASK;
  }
  const name = id.slice(id.lastIndexOf(".") + 1);
  return `${name}#${(hash >> 16n).toString(16).padStart(12, "0")}`;
};

/** The constructor name of a tag: `Circle` of `Circle#…`, `Some` of `Some`. */
export const constructorName = (t: string): string => {
  const hash = t.indexOf("#");
  return hash < 0 ? t.slice(t.lastIndexOf(".") + 1) : t.slice(0, hash);
};
