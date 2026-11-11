// The 32-bit mixing and string hash of `Hash`; the Effect library gives the same values.
const mix = (hash: number): number => {
  let value = hash;
  value ^= value >>> 16;
  value = Math.imul(value, 0x85ebca6b);
  value ^= value >>> 13;
  value = Math.imul(value, 0xc2b2ae35);
  return (value ^ (value >>> 16)) | 0;
};
export const combine = (that: number) => (self: number): number =>
  mix(Math.imul(self, 0x9e3779b1) + Math.imul(that, 0x85ebca6b));
export const optimize = (hash: number): number => (hash & 0xbfffffff) | ((hash >>> 1) & 0x40000000);
export const string = (value: string): number => {
  let hash = 5381;
  let index = value.length;
  while (index) hash = (hash * 33) ^ value.charCodeAt(--index);
  return optimize(hash);
};
