// Wasm Component binary section-1 extraction and Canonical ABI linear memory primitives.

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder("utf-8", { fatal: true });

const COMPONENT_HEADER = [0x00, 0x61, 0x73, 0x6d, 0x0d, 0x00, 0x01, 0x00];
const CORE_MODULE_HEADER = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

const matchesHeader = (bytes: Uint8Array, offset: number, header: ReadonlyArray<number>): boolean => {
  if (offset + header.length > bytes.length) return false;
  for (let i = 0; i < header.length; i++) {
    if (bytes[offset + i] !== header[i]) return false;
  }
  return true;
};

/** Extracts the core WebAssembly module (section id 1) from a Wasm Component binary. */
export const extractCoreModule = (componentBytes: Uint8Array): Uint8Array => {
  if (!matchesHeader(componentBytes, 0, COMPONENT_HEADER)) {
    throw new Error("Invalid WebAssembly component binary header");
  }
  let pos = 8;
  const readLebU32 = (): number => {
    let result = 0;
    let shift = 0;
    while (pos < componentBytes.length) {
      const byte = componentBytes[pos++]!;
      result |= (byte & 0x7f) << shift;
      if ((byte & 0x80) === 0) return result >>> 0;
      shift += 7;
      if (shift > 35) throw new Error("Malformed LEB128 section length in component binary");
    }
    throw new Error("Unexpected end of WebAssembly component binary");
  };
  while (pos < componentBytes.length) {
    const sectionId = componentBytes[pos++]!;
    const length = readLebU32();
    const end = pos + length;
    if (end > componentBytes.length) {
      throw new Error("Truncated section in WebAssembly component binary");
    }
    if (sectionId === 1 && matchesHeader(componentBytes, pos, CORE_MODULE_HEADER)) {
      return componentBytes.slice(pos, end);
    }
    pos = end;
  }
  throw new Error("WebAssembly component contains no core module section");
};

// ---------------------------------------------------------------------------
// Declarative Canonical ABI (matching wit-bindgen 0.63.0 linear memory layout)
// ---------------------------------------------------------------------------

export interface CabiMemory {
  readonly memory: WebAssembly.Memory;
  readonly realloc: (oldPtr: number, oldSize: number, align: number, newSize: number) => number;
  alloc(size: number, align: number): number;
  view(): DataView;
  bytes(): Uint8Array;
}

export const makeMemory = (
  memory: WebAssembly.Memory,
  realloc: (oldPtr: number, oldSize: number, align: number, newSize: number) => number,
): CabiMemory => ({
  memory,
  realloc,
  alloc(size: number, align: number): number {
    if (size === 0) return align;
    const ptr = realloc(0, 0, align, size) >>> 0;
    if (ptr === 0) throw new Error("Guest cabi_realloc returned null pointer");
    return ptr;
  },
  view(): DataView {
    return new DataView(memory.buffer);
  },
  bytes(): Uint8Array {
    return new Uint8Array(memory.buffer);
  },
});

export const alignTo = (offset: number, align: number): number => (offset + align - 1) & ~(align - 1);

export interface Codec<T> {
  readonly size: number;
  readonly align: number;
  lower(mem: CabiMemory, ptr: number, value: T): void;
  lift(mem: CabiMemory, ptr: number): T;
}

export const boolCodec: Codec<boolean> = {
  size: 1,
  align: 1,
  lower(mem, ptr, value) {
    mem.view().setUint8(ptr, value ? 1 : 0);
  },
  lift(mem, ptr) {
    return mem.view().getUint8(ptr) !== 0;
  },
};

export const u32Codec: Codec<number> = {
  size: 4,
  align: 4,
  lower(mem, ptr, value) {
    mem.view().setUint32(ptr, value >>> 0, true);
  },
  lift(mem, ptr) {
    return mem.view().getUint32(ptr, true);
  },
};

export const u64Codec: Codec<number> = {
  size: 8,
  align: 8,
  lower(mem, ptr, value) {
    mem.view().setBigUint64(ptr, BigInt(value), true);
  },
  lift(mem, ptr) {
    return Number(mem.view().getBigUint64(ptr, true));
  },
};

export const strCodec: Codec<string> = {
  size: 8,
  align: 4,
  lower(mem, ptr, value) {
    const encoded = textEncoder.encode(value);
    const dataPtr = encoded.length === 0 ? 0 : mem.alloc(encoded.length, 1);
    if (encoded.length > 0) mem.bytes().set(encoded, dataPtr);
    const v = mem.view();
    v.setUint32(ptr, dataPtr, true);
    v.setUint32(ptr + 4, encoded.length, true);
  },
  lift(mem, ptr) {
    const v = mem.view();
    const dataPtr = v.getUint32(ptr, true);
    const len = v.getUint32(ptr + 4, true);
    return textDecoder.decode(mem.bytes().subarray(dataPtr, dataPtr + len));
  },
};

export const bytesCodec: Codec<Uint8Array> = {
  size: 8,
  align: 4,
  lower(mem, ptr, value) {
    const dataPtr = value.length === 0 ? 0 : mem.alloc(value.length, 1);
    if (value.length > 0) mem.bytes().set(value, dataPtr);
    const v = mem.view();
    v.setUint32(ptr, dataPtr, true);
    v.setUint32(ptr + 4, value.length, true);
  },
  lift(mem, ptr) {
    const v = mem.view();
    const dataPtr = v.getUint32(ptr, true);
    const len = v.getUint32(ptr + 4, true);
    return mem.bytes().slice(dataPtr, dataPtr + len);
  },
};

export const enumCodec = <const T extends ReadonlyArray<string>>(variants: T): Codec<T[number]> => ({
  size: 1,
  align: 1,
  lower(mem, ptr, value) {
    const idx = variants.indexOf(value);
    if (idx < 0) throw new Error(`Invalid enum variant: ${String(value)}`);
    mem.view().setUint8(ptr, idx);
  },
  lift(mem, ptr) {
    const idx = mem.view().getUint8(ptr);
    const name = variants[idx];
    if (name === undefined) throw new Error(`Invalid enum discriminant: ${idx}`);
    return name;
  },
});

export const listCodec = <T>(elem: Codec<T>): Codec<ReadonlyArray<T>> => ({
  size: 8,
  align: 4,
  lower(mem, ptr, items) {
    const dataPtr = items.length === 0 ? 0 : mem.alloc(items.length * elem.size, elem.align);
    for (let i = 0; i < items.length; i++) {
      elem.lower(mem, dataPtr + i * elem.size, items[i]!);
    }
    const v = mem.view();
    v.setUint32(ptr, dataPtr, true);
    v.setUint32(ptr + 4, items.length, true);
  },
  lift(mem, ptr) {
    const v = mem.view();
    const dataPtr = v.getUint32(ptr, true);
    const len = v.getUint32(ptr + 4, true);
    const out: Array<T> = [];
    for (let i = 0; i < len; i++) {
      out.push(elem.lift(mem, dataPtr + i * elem.size));
    }
    return out;
  },
});

export const optionCodec = <T>(inner: Codec<T>): Codec<T | null> => {
  const align = Math.max(1, inner.align);
  const payloadOffset = alignTo(1, inner.align);
  const size = alignTo(payloadOffset + inner.size, align);
  return {
    size,
    align,
    lower(mem, ptr, value) {
      if (value === null) {
        mem.view().setUint8(ptr, 0);
      } else {
        mem.view().setUint8(ptr, 1);
        inner.lower(mem, ptr + payloadOffset, value);
      }
    },
    lift(mem, ptr) {
      const tag = mem.view().getUint8(ptr);
      if (tag === 0) return null;
      if (tag === 1) return inner.lift(mem, ptr + payloadOffset);
      throw new Error(`Invalid option discriminant: ${tag}`);
    },
  };
};

export const resultCodec = <Ok, Err>(
  okCodec: Codec<Ok>,
  errCodec: Codec<Err>,
): Codec<{ readonly ok: true; readonly value: Ok } | { readonly ok: false; readonly error: Err }> => {
  const payloadAlign = Math.max(1, okCodec.align, errCodec.align);
  const payloadOffset = alignTo(1, payloadAlign);
  const size = alignTo(payloadOffset + Math.max(okCodec.size, errCodec.size), payloadAlign);
  return {
    size,
    align: payloadAlign,
    lower(mem, ptr, value) {
      if (value.ok) {
        mem.view().setUint8(ptr, 0);
        okCodec.lower(mem, ptr + payloadOffset, value.value);
      } else {
        mem.view().setUint8(ptr, 1);
        errCodec.lower(mem, ptr + payloadOffset, value.error);
      }
    },
    lift(mem, ptr) {
      const tag = mem.view().getUint8(ptr);
      if (tag === 0) return { ok: true, value: okCodec.lift(mem, ptr + payloadOffset) };
      if (tag === 1) return { ok: false, error: errCodec.lift(mem, ptr + payloadOffset) };
      throw new Error(`Invalid result discriminant: ${tag}`);
    },
  };
};

export type FieldTuple = readonly [string, Codec<unknown>];

export const recordCodec = <T>(fields: ReadonlyArray<FieldTuple>): Codec<T> => {
  let align = 1;
  let offset = 0;
  const layout: Array<{ readonly key: string; readonly offset: number; readonly codec: Codec<unknown> }> = [];
  for (const [key, codec] of fields) {
    align = Math.max(align, codec.align);
    offset = alignTo(offset, codec.align);
    layout.push({ key, offset, codec });
    offset += codec.size;
  }
  const size = alignTo(offset, align);
  return {
    size,
    align,
    lower(mem, ptr, value) {
      const obj = value as Record<string, unknown>;
      for (const field of layout) {
        field.codec.lower(mem, ptr + field.offset, obj[field.key]);
      }
    },
    lift(mem, ptr) {
      const out: Record<string, unknown> = {};
      for (const field of layout) {
        out[field.key] = field.codec.lift(mem, ptr + field.offset);
      }
      return out as T;
    },
  };
};
