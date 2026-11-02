// Effect's `Crypto` for the bindings of the standard library: one Effect
// `Crypto` made over the platform's Web Crypto (`globalThis.crypto`, in Node
// and in browsers), behind the operations of the Flick service
// `Crypto.Crypto`. A `PlatformError` becomes a `Crypto.CryptoError`.
import { Crypto, Effect, Option, PlatformError, Result } from "effect";
import { Base64Url } from "effect/encoding";

import type { Tagged } from "../data.ts";
import { constructorName, tag } from "../tags.ts";

export * from "effect/Crypto";

const CRYPTO_ERROR = /*#__PURE__*/ tag("Crypto.CryptoError.CryptoError");

type CryptoError = Tagged<typeof CRYPTO_ERROR, string>;

const toError = (e: PlatformError.PlatformError): CryptoError => ({ _tag: CRYPTO_ERROR, _0: e.message });

// `getRandomValues` fills at most 65536 bytes a call
const CHUNK = 65536;

const randomBytes = (size: number): Uint8Array => {
  const out = new Uint8Array(size);
  for (let i = 0; i < size; i += CHUNK) globalThis.crypto.getRandomValues(out.subarray(i, Math.min(size, i + CHUNK)));
  return out;
};

const digest = (algorithm: Crypto.DigestAlgorithm, data: Uint8Array): Effect.Effect<Uint8Array, PlatformError.PlatformError> =>
  Effect.tryPromise({
    try: async () => new Uint8Array(await globalThis.crypto.subtle.digest(algorithm, Uint8Array.from(data))),
    catch: (cause) => PlatformError.systemError({ _tag: "Unknown", module: "Crypto", method: "digest", cause }),
  });

const system: Crypto.Crypto = /*#__PURE__*/ Crypto.make({ randomBytes, digest });

const ALGORITHMS: Record<string, Crypto.DigestAlgorithm> = {
  Sha1: "SHA-1",
  Sha256: "SHA-256",
  Sha384: "SHA-384",
  Sha512: "SHA-512",
};

const bytes = (e: Effect.Effect<Uint8Array, PlatformError.PlatformError>): Effect.Effect<Array<number>, CryptoError> =>
  e.pipe(
    Effect.map((b) => Array.from(b)),
    Effect.mapError(toError),
  );

const text = (e: Effect.Effect<string, PlatformError.PlatformError>): Effect.Effect<string, CryptoError> =>
  Effect.mapError(e, toError);

export const flickRandomBytes = (size: number): Effect.Effect<Array<number>, CryptoError> => bytes(system.randomBytes(size));

export const flickDigest = (
  algorithm: Readonly<{ _tag: string }>,
  data: ReadonlyArray<number>,
): Effect.Effect<Array<number>, CryptoError> =>
  bytes(system.digest(ALGORITHMS[constructorName(algorithm._tag)] ?? "SHA-256", Uint8Array.from(data)));

export const flickRandom = (): Effect.Effect<number> => system.random;
export const flickRandomBoolean = (): Effect.Effect<boolean> => system.randomBoolean;
// Flick's `Int` is 32-bit, so the draw is over that range rather than the safe integers.
export const flickRandomInt = (): Effect.Effect<number> => system.randomIntBetween(-0x80000000, 0x7fffffff);
export const flickRandomBetween = (min: number, max: number): Effect.Effect<number> => system.randomBetween(min, max);
export const flickRandomIntBetween = (min: number, max: number): Effect.Effect<number> => system.randomIntBetween(min, max);
export const flickRandomShuffle = <A>(xs: ReadonlyArray<A>): Effect.Effect<Array<A>> => system.randomShuffle(xs);
export const flickRandomUuidV4 = (): Effect.Effect<string, CryptoError> => text(system.randomUUIDv4());
export const flickRandomUuidV7 = (): Effect.Effect<string, CryptoError> => text(system.randomUUIDv7());
export const flickRandomUlid = (): Effect.Effect<string, CryptoError> => text(system.randomULID);

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
