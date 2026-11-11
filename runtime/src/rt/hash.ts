// `hash` of the built-in instances, computed as Go's `flickrt` computes it: a
// seed draws the same `Gen.function` on every runtime.
const optimize = (n: number): number => (n & 0xbfffffff) | ((n >>> 1) & 0x40000000);

/** The 32-bit MurmurHash3 finalizer. */
const mix = (h: number): number => {
  let x = h ^ (h >>> 16);
  x = Math.imul(x, 0x85ebca6b);
  x ^= x >>> 13;
  x = Math.imul(x, 0xc2b2ae35);
  return x ^ (x >>> 16);
};

const combine = (a: number, b: number): number => mix(Math.imul(a, 0x9e3779b1) + Math.imul(b, 0x85ebca6b));

const float64 = /*#__PURE__*/ new DataView(new ArrayBuffer(8));

const hashNumber = (n: number): number => {
  const h = n | 0;
  if (h === n) return optimize(h);
  float64.setFloat64(0, Number.isNaN(n) ? NaN : n);
  return optimize(combine(float64.getInt32(0), float64.getInt32(4)));
};

/** djb2 over the UTF-16 code units, from the last. */
const hashString = (s: string): number => {
  let h = 5381;
  for (let i = s.length - 1; i >= 0; i--) h = (h * 33) ^ s.charCodeAt(i);
  return optimize(h);
};

/** A hash consistent with `equals`: of a primitive, a list, or the fields of a record or a value of a type, in any order. */
export const hash = (a: unknown): number => {
  switch (typeof a) {
    case "number":
      return hashNumber(a);
    case "bigint":
      return hashString(a.toString(10));
    case "string":
      return hashString(a);
    case "object": {
      if (a === null) break;
      if (Array.isArray(a)) return optimize(a.reduce((h: number, x: unknown) => combine(h, hash(x)), 6151));
      let h = 12289;
      for (const key of Object.keys(a)) h ^= combine(hashString(key), hash(Reflect.get(a, key)));
      return optimize(h);
    }
    default:
  }
  return optimize(mix(hashString(String(a))));
};
