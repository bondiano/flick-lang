import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  BackendComponent,
  base64ToBytes,
  bytesToBase64,
  extractCoreModule,
  sha256Hex,
  type BackendPin,
  type BuildRequest,
} from "../src/playground/backend-component.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../..");
const wasmPath = path.join(repoRoot, "backends/js/package/component.wasm");
const pinPath = path.join(repoRoot, "backends/js/package/backend.json");
const playgroundWasmDir = path.join(repoRoot, "docs/src/playground/wasm");

const componentBytes = new Uint8Array(readFileSync(wasmPath));
const componentPin = JSON.parse(readFileSync(pinPath, "utf8")) as BackendPin;

const loadPlaygroundWasm = async (): Promise<{
  readonly Session: new () => {
    update(source: string): string;
    prepare_emit(profile: string, includeDeclarations: boolean): string;
    finish_emit(profile: string, includeDeclarations: boolean, resultJson: string): string;
  };
}> => {
  const jsUrl = new URL("../src/playground/wasm/flick_wasm.js", import.meta.url).href;
  const mod = await import(jsUrl) as {
    readonly default: (input: { readonly module_or_path: Uint8Array }) => Promise<unknown>;
    readonly Session: new () => {
      update(source: string): string;
      prepare_emit(profile: string, includeDeclarations: boolean): string;
      finish_emit(profile: string, includeDeclarations: boolean, resultJson: string): string;
    };
  };
  const bgBytes = new Uint8Array(readFileSync(path.join(playgroundWasmDir, "flick_wasm_bg.wasm")));
  await mod.default({ module_or_path: bgBytes });
  return mod;
};

test("official JS component loads with verified SHA-256 and lifts full descriptor", async () => {
  const backend = await BackendComponent.load(componentBytes, componentPin);
  assert.deepEqual(backend.descriptor.identity, componentPin.identity);
  assert.ok(backend.descriptor.environments.some((e) => e.backend === "js" && e.platform === "browser"));
  assert.deepEqual(backend.descriptor.profiles, ["Debug", "Release"]);
  assert.ok(backend.descriptor.projections.some((p) => p.name === "flick.javascript" && p.version === "0.1.0"));
  assert.ok(backend.descriptor.value_abi !== null);
  assert.equal(backend.descriptor.value_abi.abi.name, "flick.javascript-ffi");
});

test("tampered component bytes are refused with explicit SHA-256 mismatch", async () => {
  const tampered = componentBytes.slice();
  tampered[tampered.length - 1] = (tampered[tampered.length - 1]! ^ 0xff) & 0xff;
  await assert.rejects(
    BackendComponent.load(tampered, componentPin),
    /SHA-256 mismatch/,
  );
});

test("mismatched identity pin is refused even when SHA-256 matches", async () => {
  const wrongIdentity: BackendPin = {
    identity: { id: "flick/other", version: componentPin.identity.version },
    sha256: componentPin.sha256,
  };
  await assert.rejects(
    BackendComponent.load(componentBytes, wrongIdentity),
    /identity mismatch/,
  );
});

test("malformed component header and forbidden imports are refused", async () => {
  assert.throws(
    () => extractCoreModule(new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7])),
    /Invalid WebAssembly component binary header/,
  );
  // Minimal core Wasm module importing `wasi:filesystem/types#open` -> `() -> ()`
  const coreModule = new Uint8Array([
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    // type section: 1 type () -> ()
    0x01, 0x04, 0x01, 0x60, 0x00, 0x00,
    // import section: "wasi:fs" "open" func 0
    0x02, 0x10, 0x01,
    0x07, ...new TextEncoder().encode("wasi:fs"),
    0x04, ...new TextEncoder().encode("open"),
    0x00, 0x00,
  ]);
  const component = new Uint8Array([
    0x00, 0x61, 0x73, 0x6d, 0x0d, 0x00, 0x01, 0x00,
    0x01, coreModule.length,
    ...coreModule,
  ]);
  const sha = await sha256Hex(component);
  await assert.rejects(
    BackendComponent.load(component, { identity: componentPin.identity, sha256: sha }),
    /forbidden import wasi:fs#open/,
  );
});

test("declared input manifest refuses undeclared, unavailable, and tampered inputs", async () => {
  const good = new TextEncoder().encode("hello");
  const goodSha = await sha256Hex(good);
  const reader = BackendComponent.createManifestReader(
    [
      { id: "source/0", kind: "Source", sha256: goodSha, size: good.byteLength },
      { id: "source/missing", kind: "Source", sha256: goodSha, size: good.byteLength },
      { id: "source/tampered", kind: "Source", sha256: goodSha, size: good.byteLength },
    ],
    new Map([
      ["source/0", good],
      ["source/tampered", new TextEncoder().encode("world")],
    ]),
  );
  assert.deepEqual(await reader("source/0"), { ok: true, bytes: good });
  assert.deepEqual(await reader("undeclared/secret"), { ok: false, code: 0 });
  assert.deepEqual(await reader("source/missing"), { ok: false, code: 1 });
  assert.deepEqual(await reader("source/tampered"), { ok: false, code: 2 });
});

test("playground prepare_emit -> component.wasm -> finish_emit round-trips debug, release, and guest diagnostics", async () => {
  if (!existsSync(path.join(playgroundWasmDir, "flick_wasm_bg.wasm"))) {
    // Only skipped when running `docs` tests standalone without `just docs-wasm`.
    return;
  }
  const { Session } = await loadPlaygroundWasm();
  const backend = await BackendComponent.load(componentBytes, componentPin);

  const runRoundTrip = async (source: string, profile: "debug" | "release", includeDeclarations = false) => {
    const session = new Session();
    session.update(source);
    const prepared = JSON.parse(session.prepare_emit(profile, includeDeclarations)) as
      | { readonly status: "done"; readonly emitted: Record<string, unknown> }
      | {
          readonly status: "ready";
          readonly pin: BackendPin;
          readonly request: Omit<BuildRequest, "program"> & { readonly program: ReadonlyArray<number> };
          readonly program_b64: string;
          readonly inputs: Readonly<Record<string, string>>;
        };
    if (prepared.status === "done") return prepared.emitted;

    const request: BuildRequest = {
      ...prepared.request,
      program: base64ToBytes(prepared.program_b64),
    };
    const inputs = new Map<string, Uint8Array>();
    for (const [id, b64] of Object.entries(prepared.inputs)) {
      inputs.set(id, base64ToBytes(b64));
    }
    const checked = await backend.checkTarget(request, inputs);
    let reply: unknown;
    if (!checked.ok) {
      reply = { status: "err", descriptor: backend.descriptor, diagnostics: checked.diagnostics };
    } else {
      const generated = await backend.generate(request, inputs);
      if (!generated.ok) {
        reply = { status: "err", descriptor: backend.descriptor, diagnostics: generated.diagnostics };
      } else {
        reply = {
          status: "ok",
          descriptor: backend.descriptor,
          result: {
            ...generated.value,
            artifacts: generated.value.artifacts.map((a) => ({ id: a.id, path: a.path, kind: a.kind, bytes: [] })),
          },
          artifact_bytes_b64: generated.value.artifacts.map((a) => bytesToBase64(a.bytes)),
        };
      }
    }
    return JSON.parse(session.finish_emit(profile, includeDeclarations, JSON.stringify(reply))) as Record<string, unknown>;
  };

  const program = [
    "module Main exposing (answer, main)",
    "answer : Int",
    "let answer = 21 * 2",
    "main : Unit ->{Host.Console} Unit",
    "let main () = Platform.stdout_line (show answer)",
  ].join("\n") + "\n";

  for (const profile of ["debug", "release"] as const) {
    const emitted = await runRoundTrip(program, profile, true);
    assert.deepEqual(emitted.diagnostics, []);
    assert.equal(emitted.entry, "flick-main.js");
    assert.equal(typeof emitted.js, "string");
    assert.equal(typeof emitted.dts, "string");
    assert.equal(emitted.declaration_entry, "Main.d.ts");
    const files = emitted.files as Record<string, string>;
    assert.ok(files["Main.js"]);
    assert.ok(files["flick-main.js"]);
    assert.ok(files["_flick/Platform.js"]);
    if (profile === "release") {
      // Constant folding in the release optimizer inside component.wasm folds 21 * 2 -> 42.
      assert.match(files["Main.js"]!, /\b42\b/);
    }
  }

  // Guest diagnostic mapping: invalid inline JS syntax is rejected by `component.wasm`
  // with `FLICK-F011` and mapped back to UTF-16 offsets after a surrogate-pair emoji.
  const badInline = [
    "module Main exposing (bad)",
    "// привет 🚀",
    "bad : Int -> Int",
    "foreign pure bad from inline \"(x) => (\"",
  ].join("\n") + "\n";
  const rejected = await runRoundTrip(badInline, "debug");
  assert.equal(rejected.js, null);
  const diags = rejected.diagnostics as Array<{ readonly code: string; readonly from?: number; readonly to?: number }>;
  const f011 = diags.find((d) => d.code === "FLICK-F011");
  assert.ok(f011, `expected FLICK-F011 guest diagnostic, got ${JSON.stringify(diags)}`);
  assert.equal(typeof f011.from, "number");
  assert.equal(typeof f011.to, "number");
  assert.ok(f011.from! > 0 && f011.to! >= f011.from!);
});
