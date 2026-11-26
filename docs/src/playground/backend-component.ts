// Browser host for the pinned official `flick/js` WebAssembly component (`flick:backend@0.1.0`).
//
// Verifies the component's SHA-256 against its pin before instantiation, extracts
// the core WebAssembly module (section id 1) from the Wasm Component binary,
// enforces an allowlist of imports (only `flick:backend/inputs@0.1.0#read`, zero
// WASI/filesystem/network/DOM authority), and runs every `describe`, `check-target`,
// and `generate` call in a fresh WebAssembly instance.

export * from "./component-types.ts";
export { extractCoreModule } from "./component-abi.ts";

import { extractCoreModule, makeMemory, type CabiMemory } from "./component-abi.ts";
import {
  buildRequestCodec,
  checkTargetResultCodec,
  descriptorCodec,
  generateResultCodec,
} from "./component-codecs.ts";
import {
  sha256Hex,
  type BackendCallResult,
  type BackendPin,
  type BuildRequest,
  type BuildResult,
  type Descriptor,
  type InputDecl,
  type TargetSupport,
} from "./component-types.ts";

const textDecoder = new TextDecoder("utf-8", { fatal: true });

interface CoreExports {
  readonly memory: WebAssembly.Memory;
  readonly cabi_realloc: (oldPtr: number, oldSize: number, align: number, newSize: number) => number;
  readonly describe: () => number;
  readonly cabi_post_describe: (ptr: number) => void;
  readonly "check-target": (reqPtr: number) => number;
  readonly "cabi_post_check-target": (ptr: number) => void;
  readonly generate: (reqPtr: number) => number;
  readonly cabi_post_generate: (ptr: number) => void;
}

const INPUT_ERROR_UNDECLARED = 0;
const INPUT_ERROR_UNAVAILABLE = 1;
const INPUT_ERROR_DIGEST_MISMATCH = 2;

const MAX_INPUT_CALLS = 16_384;
const MAX_INPUT_TRANSFER_BYTES = 256 << 20;

export class BackendComponent {
  readonly pin: BackendPin;
  readonly descriptor: Descriptor;
  readonly #module: WebAssembly.Module;

  private constructor(module: WebAssembly.Module, pin: BackendPin, descriptor: Descriptor) {
    this.#module = module;
    this.pin = pin;
    this.descriptor = descriptor;
  }

  /**
   * Verifies `bytes` against `pin.sha256`, enforces the `inputs.read`-only import
   * allowlist, compiles the core module, and verifies `describe().identity === pin.identity`.
   * Refuses mismatches explicitly with no fallback.
   */
  static async load(bytes: Uint8Array, pin: BackendPin): Promise<BackendComponent> {
    if (!/^[0-9a-f]{64}$/.test(pin.sha256)) {
      throw new Error(`Invalid backend pin SHA-256: ${pin.sha256}`);
    }
    const actualSha = await sha256Hex(bytes);
    if (actualSha !== pin.sha256) {
      throw new Error(
        `Backend component ${pin.identity.id}@${pin.identity.version} SHA-256 mismatch: expected ${pin.sha256}, got ${actualSha}`,
      );
    }
    const coreBytes = extractCoreModule(bytes);
    const coreCopy = new Uint8Array(coreBytes.byteLength);
    coreCopy.set(coreBytes);
    const module = await WebAssembly.compile(coreCopy);
    for (const imp of WebAssembly.Module.imports(module)) {
      if (imp.module !== "flick:backend/inputs@0.1.0" || imp.name !== "read" || imp.kind !== "function") {
        throw new Error(`Backend component requests forbidden import ${imp.module}#${imp.name} (${imp.kind})`);
      }
    }
    const exportNames = new Set(WebAssembly.Module.exports(module).map((e) => e.name));
    for (const required of [
      "memory",
      "cabi_realloc",
      "describe",
      "cabi_post_describe",
      "check-target",
      "cabi_post_check-target",
      "generate",
      "cabi_post_generate",
    ]) {
      if (!exportNames.has(required)) {
        throw new Error(`Backend component is missing required export \`${required}\``);
      }
    }
    const descriptor = await BackendComponent.#callDescribe(module);
    if (
      descriptor.identity.id !== pin.identity.id ||
      descriptor.identity.version !== pin.identity.version
    ) {
      throw new Error(
        `Backend component identity mismatch: expected ${pin.identity.id}@${pin.identity.version}, got ${descriptor.identity.id}@${descriptor.identity.version}`,
      );
    }
    return new BackendComponent(module, pin, descriptor);
  }

  static async #instantiate(
    module: WebAssembly.Module,
    readInput: (id: string) => { readonly ok: true; readonly bytes: Uint8Array } | { readonly ok: false; readonly code: number },
  ): Promise<{ readonly exports: CoreExports; readonly mem: CabiMemory }> {
    let mem: CabiMemory | undefined;
    let calls = 0;
    let transferred = 0;
    const imports: WebAssembly.Imports = {
      "flick:backend/inputs@0.1.0": {
        read(idPtr: number, idLen: number, retPtr: number): void {
          if (mem === undefined) throw new Error("Guest called inputs.read before initialization");
          if (++calls > MAX_INPUT_CALLS) {
            throw new Error("Backend exceeded maximum inputs.read call count");
          }
          const id = textDecoder.decode(mem.bytes().subarray(idPtr, idPtr + idLen));
          const answer = readInput(id);
          if (!answer.ok) {
            const view = mem.view();
            view.setUint8(retPtr, 1);
            view.setUint8(retPtr + 4, answer.code);
            return;
          }
          transferred += answer.bytes.length;
          if (transferred > MAX_INPUT_TRANSFER_BYTES) {
            throw new Error("Backend exceeded maximum input transfer limit");
          }
          const dataPtr = answer.bytes.length === 0 ? 0 : mem.alloc(answer.bytes.length, 1);
          if (answer.bytes.length > 0) mem.bytes().set(answer.bytes, dataPtr);
          const view = mem.view();
          view.setUint8(retPtr, 0);
          view.setUint32(retPtr + 4, dataPtr, true);
          view.setUint32(retPtr + 8, answer.bytes.length, true);
        },
      },
    };
    const instance = await WebAssembly.instantiate(module, imports);
    const exports = instance.exports as unknown as CoreExports;
    mem = makeMemory(exports.memory, exports.cabi_realloc);
    return { exports, mem };
  }

  static async #callDescribe(module: WebAssembly.Module): Promise<Descriptor> {
    const { exports, mem } = await BackendComponent.#instantiate(module, () => ({
      ok: false,
      code: INPUT_ERROR_UNDECLARED,
    }));
    const retPtr = exports.describe() >>> 0;
    const descriptor = descriptorCodec.lift(mem, retPtr);
    exports.cabi_post_describe(retPtr);
    return descriptor;
  }

  async #snapshotInputs(
    request: BuildRequest,
    inputs: ReadonlyMap<string, Uint8Array>,
  ): Promise<ReadonlyMap<string, Uint8Array>> {
    if (
      request.backend.identity.id !== this.pin.identity.id ||
      request.backend.identity.version !== this.pin.identity.version ||
      request.backend.sha256 !== this.pin.sha256
    ) {
      throw new Error(
        `Request backend pin ${request.backend.identity.id}@${request.backend.identity.version}#${request.backend.sha256} differs from loaded component ${this.pin.identity.id}@${this.pin.identity.version}#${this.pin.sha256}`,
      );
    }
    const verified = new Map<string, Uint8Array>();
    for (const decl of request.inputs) {
      if (decl.id.length === 0 || verified.has(decl.id)) {
        throw new Error(`Invalid or duplicate declared input id \`${decl.id}\``);
      }
      const bytes = inputs.get(decl.id);
      if (bytes === undefined) {
        throw new Error(`Declared input \`${decl.id}\` is unavailable`);
      }
      if (bytes.byteLength !== decl.size) {
        throw new Error(`Declared input \`${decl.id}\` size mismatch: expected ${decl.size}, got ${bytes.byteLength}`);
      }
      const actual = await sha256Hex(bytes);
      if (actual !== decl.sha256) {
        throw new Error(`Declared input \`${decl.id}\` SHA-256 mismatch: expected ${decl.sha256}, got ${actual}`);
      }
      verified.set(decl.id, bytes);
    }
    return verified;
  }

  #readerFor(
    verified: ReadonlyMap<string, Uint8Array>,
  ): (id: string) => { readonly ok: true; readonly bytes: Uint8Array } | { readonly ok: false; readonly code: number } {
    return (id: string) => {
      const bytes = verified.get(id);
      if (bytes === undefined) return { ok: false, code: INPUT_ERROR_UNDECLARED };
      return { ok: true, bytes };
    };
  }

  /** Exposed for unit tests verifying input error codes directly against the manifest boundary. */
  static createManifestReader(
    manifest: ReadonlyArray<InputDecl>,
    inputs: ReadonlyMap<string, Uint8Array>,
  ): (id: string) => Promise<{ readonly ok: true; readonly bytes: Uint8Array } | { readonly ok: false; readonly code: number }> {
    const byId = new Map(manifest.map((d) => [d.id, d]));
    return async (id: string) => {
      const decl = byId.get(id);
      if (decl === undefined) return { ok: false, code: INPUT_ERROR_UNDECLARED };
      const bytes = inputs.get(id);
      if (bytes === undefined) return { ok: false, code: INPUT_ERROR_UNAVAILABLE };
      if (bytes.byteLength !== decl.size || (await sha256Hex(bytes)) !== decl.sha256) {
        return { ok: false, code: INPUT_ERROR_DIGEST_MISMATCH };
      }
      return { ok: true, bytes };
    };
  }

  async checkTarget(
    request: BuildRequest,
    inputs: ReadonlyMap<string, Uint8Array>,
  ): Promise<BackendCallResult<TargetSupport>> {
    const verified = await this.#snapshotInputs(request, inputs);
    const { exports, mem } = await BackendComponent.#instantiate(this.#module, this.#readerFor(verified));
    const reqPtr = mem.alloc(buildRequestCodec.size, buildRequestCodec.align);
    buildRequestCodec.lower(mem, reqPtr, request);
    const retPtr = exports["check-target"](reqPtr) >>> 0;
    const res = checkTargetResultCodec.lift(mem, retPtr);
    exports["cabi_post_check-target"](retPtr);
    return res.ok ? { ok: true, value: res.value } : { ok: false, diagnostics: res.error };
  }

  async generate(
    request: BuildRequest,
    inputs: ReadonlyMap<string, Uint8Array>,
  ): Promise<BackendCallResult<BuildResult>> {
    const verified = await this.#snapshotInputs(request, inputs);
    const { exports, mem } = await BackendComponent.#instantiate(this.#module, this.#readerFor(verified));
    const reqPtr = mem.alloc(buildRequestCodec.size, buildRequestCodec.align);
    buildRequestCodec.lower(mem, reqPtr, request);
    const retPtr = exports.generate(reqPtr) >>> 0;
    const res = generateResultCodec.lift(mem, retPtr);
    exports.cabi_post_generate(retPtr);
    return res.ok ? { ok: true, value: res.value } : { ok: false, diagnostics: res.error };
  }
}
