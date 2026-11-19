import { item } from "../internal.ts";
import { err, none, ok, some, type Option, type Result } from "./data.ts";

const emptyData = /*#__PURE__*/ new Uint8Array(0);
const utf8Encoder = /*#__PURE__*/ new TextEncoder();
const utf8Decoder = /*#__PURE__*/ new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

/** Packed storage owned by the runtime. A slice may share that storage. */
class PackedBytes {
  #data = emptyData;

  static readonly empty = /*#__PURE__*/ new PackedBytes();

  /** Takes ownership of storage allocated by this module. */
  static #owned(data: Uint8Array<ArrayBuffer>): PackedBytes {
    if (data.length === 0) return PackedBytes.empty;
    const value = new PackedBytes();
    value.#data = data;
    return value;
  }

  static length(this: void, value: PackedBytes): number {
    return value.#data.length;
  }

  static get(this: void, index: number, value: PackedBytes): Option<number> {
    const byte = value.#data[index];
    return byte === undefined ? none : some(byte);
  }

  static slice(this: void, start: number, end: number, value: PackedBytes): Option<PackedBytes> {
    if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || start > end || end > value.#data.length) {
      return none;
    }
    if (start === end) return some(PackedBytes.empty);
    return some(PackedBytes.#owned(value.#data.subarray(start, end)));
  }

  static fromList(this: void, list: ReadonlyArray<number>): Result<PackedBytes, readonly [number, number]> {
    const data = new Uint8Array(list.length);
    for (let index = 0; index < data.length; index++) {
      const byte = item(list, index);
      if (!Number.isInteger(byte) || byte < 0 || byte > 255) return err([index, byte]);
      data[index] = byte;
    }
    return ok(PackedBytes.#owned(data));
  }

  static toList(this: void, value: PackedBytes): Array<number> {
    return Array.from(value.#data);
  }

  static encodeUtf8(this: void, text: string): PackedBytes {
    return PackedBytes.#owned(utf8Encoder.encode(text));
  }

  static decodeUtf8(this: void, value: PackedBytes): Option<string> {
    try { return some(utf8Decoder.decode(value.#data)); }
    catch { return none; }
  }

  static encodeBase64Url(this: void, value: PackedBytes): string {
    const data = value.#data;
    // Bound the number of arguments to fromCharCode for large byte strings.
    const chunkSize = 8192;
    let binary = "";
    for (let offset = 0; offset < data.length; offset += chunkSize) {
      binary += String.fromCharCode(...data.subarray(offset, offset + chunkSize));
    }
    return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
  }

  static decodeBase64Url(this: void, text: string): Option<PackedBytes> {
    if (!/^[\w-]*={0,2}$/.test(text) || text.length % 4 === 1) return none;
    try {
      const binary = atob(text.replaceAll("-", "+").replaceAll("_", "/"));
      return some(PackedBytes.#owned(Uint8Array.from(binary, (char) => char.charCodeAt(0))));
    } catch { return none; }
  }

  static equalConstantTime(this: void, left: PackedBytes, right: PackedBytes): boolean {
    const a = left.#data;
    const b = right.#data;
    if (a.length !== b.length) return false;
    let difference = 0;
    for (let index = 0; index < a.length; index++) {
      difference |= (a[index] ?? 0) ^ (b[index] ?? 0);
    }
    return difference === 0;
  }

  ["~effect/Equal"](other: unknown): boolean {
    return other instanceof PackedBytes && PackedBytes.equalConstantTime(this, other);
  }

  ["~effect/Hash"](): number {
    return this.#data.length;
  }
}

export const bytesEmpty = PackedBytes.empty;
export const bytesLength = PackedBytes.length;
export const bytesGet = PackedBytes.get;
export const bytesSlice = PackedBytes.slice;
export const bytesFromList = PackedBytes.fromList;
export const bytesToList = PackedBytes.toList;
export const bytesEncodeUtf8 = PackedBytes.encodeUtf8;
export const bytesDecodeUtf8 = PackedBytes.decodeUtf8;
export const bytesEncodeBase64Url = PackedBytes.encodeBase64Url;
export const bytesDecodeBase64Url = PackedBytes.decodeBase64Url;
export const bytesEqualConstantTime = PackedBytes.equalConstantTime;
