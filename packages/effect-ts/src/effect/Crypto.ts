// The `Crypto` service of the standard library: Effect's `SecureRandom` over
// the platform's Web Crypto (`globalThis.crypto`, in Node and in browsers),
// with digests and PBKDF2 from `crypto.subtle`, behind the operations of the
// Flick service `Crypto.Crypto`. A failure becomes a `Crypto.CryptoError`.
import { Effect, Option, Result } from "effect";
import { SecureRandom } from "effect/crypto";
import { Base64Url } from "effect/encoding";

import type { Tagged } from "../data.ts";
import { constructorName, tag } from "@flick-lang/runtime/tags";

const CRYPTO_ERROR = /*#__PURE__*/ tag("Crypto.CryptoError.CryptoError");

type CryptoError = Tagged<typeof CRYPTO_ERROR, string>;

type DigestAlgorithm = "SHA-1" | "SHA-256" | "SHA-384" | "SHA-512";

// `getRandomValues` fills at most 65536 bytes a call
const CHUNK = 65536;

const randomBytes = (size: number): Uint8Array => {
  const out = new Uint8Array(size);
  for (let i = 0; i < size; i += CHUNK) globalThis.crypto.getRandomValues(out.subarray(i, Math.min(size, i + CHUNK)));
  return out;
};

const system = /*#__PURE__*/ SecureRandom.make(randomBytes);

// A double in [0, 1) from 53 random bits, as Effect's `Crypto` drew it before 4.1.
const nextDouble = (): number => {
  const b = randomBytes(7);
  return ((b[0] ?? 0) & 0x1f) * 2 ** 48 + (b[1] ?? 0) * 2 ** 40 + (b[2] ?? 0) * 2 ** 32 + (b[3] ?? 0) * 2 ** 24 + (b[4] ?? 0) * 2 ** 16 + (b[5] ?? 0) * 2 ** 8 + (b[6] ?? 0);
};
const draw = (): number => nextDouble() / 2 ** 53;

// Effect's `nextBetween`: a draw in [min, max), the predecessor of `max` when rounding reaches it.
const between = (min: number, max: number, d: number): number => {
  const value = d * (max - min) + min;
  if (value !== max || min >= max || !Number.isFinite(max)) return value;
  if (max === 0) return -Number.MIN_VALUE;
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, max);
  const bits = view.getBigUint64(0);
  view.setBigUint64(0, max > 0 ? bits - 1n : bits + 1n);
  return view.getFloat64(0);
};

const cryptoError = (message: string): CryptoError => ({ _tag: CRYPTO_ERROR, _0: message });

const digest = (algorithm: DigestAlgorithm, data: Uint8Array): Effect.Effect<Uint8Array, CryptoError> =>
  Effect.tryPromise({
    try: async () => new Uint8Array(await globalThis.crypto.subtle.digest(algorithm, Uint8Array.from(data))),
    catch: (cause) => cryptoError(`Crypto.digest: ${cause instanceof Error ? cause.message : String(cause)}`),
  });

const ALGORITHMS: Record<string, DigestAlgorithm> = {
  Sha1: "SHA-1",
  Sha256: "SHA-256",
  Sha384: "SHA-384",
  Sha512: "SHA-512",
};

const bytes = <E>(e: Effect.Effect<Uint8Array, E>): Effect.Effect<Array<number>, E> => Effect.map(e, (b) => Array.from(b));

export const flickRandomBytes = (size: number): Effect.Effect<Array<number>, CryptoError> =>
  Number.isSafeInteger(size) && size >= 0
    ? bytes(system.randomBytes(size))
    : Effect.fail(cryptoError("Crypto.randomBytes: size must be a non-negative safe integer"));

export const flickDigest = (
  algorithm: Readonly<{ _tag: string }>,
  data: ReadonlyArray<number>,
): Effect.Effect<Array<number>, CryptoError> =>
  bytes(digest(ALGORITHMS[constructorName(algorithm._tag)] ?? "SHA-256", Uint8Array.from(data)));

export const flickPbkdf2 = (
  algorithm: Readonly<{ _tag: string }>,
  iterations: number,
  length: number,
  salt: ReadonlyArray<number>,
  password: ReadonlyArray<number>,
): Effect.Effect<Array<number>, CryptoError> =>
  iterations <= 0 || length <= 0
    ? Effect.fail(cryptoError("Crypto.pbkdf2: iterations and length must be positive"))
    : Effect.tryPromise({
        try: async () => {
          const subtle = globalThis.crypto.subtle;
          const key = await subtle.importKey("raw", Uint8Array.from(password), "PBKDF2", false, ["deriveBits"]);
          const hash = ALGORITHMS[constructorName(algorithm._tag)] ?? "SHA-256";
          const bits = await subtle.deriveBits({ name: "PBKDF2", hash, salt: Uint8Array.from(salt), iterations }, key, length * 8);
          return Array.from(new Uint8Array(bits));
        },
        catch: (cause) => cryptoError(`Crypto.pbkdf2: ${String(cause)}`),
      });

export const flickRandom = (): Effect.Effect<number> => Effect.sync(draw);
export const flickRandomBoolean = (): Effect.Effect<boolean> => Effect.sync(() => draw() > 0.5);
// Flick's `Int` is 32-bit, so the draw is over that range rather than the safe integers.
export const flickRandomInt = (): Effect.Effect<number> => system.intBetween(-0x80000000, 0x7fffffff);
export const flickRandomBetween = (min: number, max: number): Effect.Effect<number> => Effect.sync(() => between(min, max, draw()));
export const flickRandomIntBetween = (min: number, max: number): Effect.Effect<number> => system.intBetween(min, max);
export const flickRandomShuffle = <A>(xs: ReadonlyArray<A>): Effect.Effect<Array<A>> => system.shuffle(xs);
export const flickRandomUuidV4 = (): Effect.Effect<string, CryptoError> => system.uuidV4();
export const flickRandomUuidV7 = (): Effect.Effect<string, CryptoError> => system.uuidV7();
export const flickRandomUlid = (): Effect.Effect<string, CryptoError> => system.ulid;

// `Bytes`: text as UTF-8, bytes as URL-safe base64.
const UTF8 = /*#__PURE__*/ new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

export const flickUtf8Encode = (input: string): Array<number> => Array.from(new TextEncoder().encode(input));

export const flickUtf8Decode = (data: ReadonlyArray<number>): Option.Option<string> => {
  try {
    return Option.some(UTF8.decode(Uint8Array.from(data)));
  } catch {
    return Option.none();
  }
};

export const flickBase64UrlEncode = (data: ReadonlyArray<number>): string => Base64Url.encode(Uint8Array.from(data));

export const flickBase64UrlDecode = (input: string): Option.Option<Array<number>> => {
  const decoded = Base64Url.decode(input);
  return Result.isSuccess(decoded) ? Option.some(Array.from(decoded.success)) : Option.none();
};
