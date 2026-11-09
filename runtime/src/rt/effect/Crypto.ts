// Web Crypto bindings for Flick's own runtime. Byte values use dense number
// arrays, while asynchronous digest operations yield Promises to the driver.
import { none, some, type Option } from "../../core/data.ts";
import { item } from "../../internal.ts";
import { constructorName, tag } from "../../tags.ts";
import { fail, raise } from "../cause.ts";
import { foreignAsync } from "../owner.ts";

const cryptoError = (message: string): { readonly _tag: string; readonly _0: string } =>
  ({ _tag: tag("Crypto.CryptoError.CryptoError"), _0: message });
const error = (message: string): never => raise(fail(cryptoError(message)));
const algorithm = (value: { readonly _tag: string }): string => ({
  Sha1: "SHA-1", Sha256: "SHA-256", Sha384: "SHA-384", Sha512: "SHA-512",
})[constructorName(value._tag)] ?? "SHA-256";
const randomBytes = (count: number): Uint8Array => {
  const bytes = new Uint8Array(count);
  for (let offset = 0; offset < count; offset += 65536) crypto.getRandomValues(bytes.subarray(offset, Math.min(count, offset + 65536)));
  return bytes;
};
const draw = (): number => {
  const bytes = randomBytes(7);
  let value = (bytes[0] ?? 0) & 31;
  for (let index = 1; index < 7; index++) value = value * 256 + (bytes[index] ?? 0);
  return value / 2 ** 53;
};
const onError = (name: string) => (cause: unknown) =>
  cryptoError(`Crypto.${name}: ${cause instanceof Error ? cause.message : String(cause)}`);
export const flickRandomBytes = (size: number): Array<number> => {
  if (!Number.isSafeInteger(size) || size < 0) return error("Crypto.randomBytes: size must be a non-negative safe integer");
  return Array.from(randomBytes(size));
};
export const flickDigest = (name: { readonly _tag: string }, bytes: ReadonlyArray<number>) =>
  foreignAsync(() => crypto.subtle.digest(algorithm(name), Uint8Array.from(bytes)).then((result) => Array.from(new Uint8Array(result))), onError("digest"));
export const flickPbkdf2 = (
  name: { readonly _tag: string }, iterations: number, length: number,
  salt: ReadonlyArray<number>, password: ReadonlyArray<number>,
): ReturnType<typeof foreignAsync> => {
  if (!Number.isSafeInteger(iterations) || !Number.isSafeInteger(length) || iterations <= 0 || length <= 0) {
    return error("Crypto.pbkdf2: iterations and length must be positive");
  }
  const derive = async (): Promise<Array<number>> => {
    const key = await crypto.subtle.importKey("raw", Uint8Array.from(password), "PBKDF2", false, ["deriveBits"]);
    const bits = await crypto.subtle.deriveBits({ name: "PBKDF2", hash: algorithm(name), salt: Uint8Array.from(salt), iterations }, key, length * 8);
    return Array.from(new Uint8Array(bits));
  };
  return foreignAsync(derive, onError("pbkdf2"));
};
export const flickRandom = (): number => draw();
export const flickRandomBoolean = (): boolean => draw() > 0.5;
export const flickRandomInt = (): number => crypto.getRandomValues(new Int32Array(1))[0] ?? 0;
export const flickRandomBetween = (minimum: number, maximum: number): number => minimum + draw() * (maximum - minimum);
export const flickRandomIntBetween = (minimum: number, maximum: number): number =>
  Math.floor(draw() * (maximum - minimum + 1)) + minimum;
export const flickRandomShuffle = <A>(values: ReadonlyArray<A>): Array<A> => {
  const shuffled = [...values];
  for (let index = shuffled.length - 1; index > 0; index--) {
    const other = Math.floor(draw() * (index + 1));
    const first = item(shuffled, index);
    shuffled[index] = item(shuffled, other);
    shuffled[other] = first;
  }
  return shuffled;
};
const uuid = (bytes: Uint8Array): string => {
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
};
export const flickRandomUuidV4 = (): string => crypto.randomUUID();
export const flickRandomUuidV7 = (): string => {
  const bytes = randomBytes(16);
  let time = BigInt(Date.now());
  for (let index = 5; index >= 0; index--) { bytes[index] = Number(time & 255n); time >>= 8n; }
  bytes[6] = ((bytes[6] ?? 0) & 15) | 0x70;
  bytes[8] = ((bytes[8] ?? 0) & 63) | 0x80;
  return uuid(bytes);
};
export const flickRandomUlid = (): string => {
  let number = BigInt(Date.now());
  for (const byte of randomBytes(10)) number = (number << 8n) | BigInt(byte);
  const alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
  let result = "";
  for (let index = 0; index < 26; index++) { result = alphabet[Number(number & 31n)] + result; number >>= 5n; }
  return result;
};

const UTF8 = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
export const flickUtf8Encode = (value: string): Array<number> => Array.from(new TextEncoder().encode(value));
export const flickUtf8Decode = (bytes: ReadonlyArray<number>): Option<string> => {
  try { return some(UTF8.decode(Uint8Array.from(bytes))); }
  catch { return none; }
};
export const flickBase64UrlEncode = (bytes: ReadonlyArray<number>): string =>
  btoa(Array.from(bytes, (byte) => String.fromCharCode(byte)).join("")).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
export const flickBase64UrlDecode = (value: string): Option<Array<number>> => {
  if (!/^[\w-]*={0,2}$/.test(value) || value.length % 4 === 1) return none;
  try {
    const decoded = atob(value.replaceAll("-", "+").replaceAll("_", "/"));
    return some(Array.from(decoded, (char) => char.charCodeAt(0)));
  } catch { return none; }
};
