# Official Backend Component Cache-Miss Attribution & Measurements

This document records repeatable real-project (`examples/mtl`) and small-program
build measurements for the official `flick/js` and `flick/go` WebAssembly
components, attributing every sub-stage of a cache miss before changing the
component trust, validation, or determinism-replay path (issue #869 / #456).

## Reproducing these measurements

```bash
cargo build --release -p flick
pnpm -C runtime build
pnpm -C packages/effect-ts build
node tools/backend-bench/measure.mjs --samples 5
```

`tools/backend-bench/measure.mjs` measures:

1. **External process wall time and peak RSS** via GNU `time -v` **without**
   `--timings`, so timing instrumentation never perturbs process measurements.
2. **Internal stage attribution and payload byte sizes** via
   `flick build --timings --diagnostics json`, collected across 5 fresh
   processes per mode (reporting median and min–max dispersion).
3. **Artifact determinism and execution equivalence**: every scenario builds an
   isolated copy, verifies identical output directory digests on restored
   matched sources, and verifies identical execution stdout across JS RT, JS
   Effect, and Go.

## Pinned environment

- **Host OS / CPU:** Linux `6.6.87.2-microsoft-standard-WSL2` x64 — AMD Ryzen 9 9950X3D 16-Core Processor (32 threads, 47.0 GiB RAM)
- **Toolchains:** `rustc 1.95.0 (59807616e 2026-04-14)` | Node `v24.22.1` | `go1.27.3 linux/amd64`
- **Base CLI git SHA:** `735b4f38d86e9d186a563fc7cb757e27677248d6` (`target/release/flick`)
- **Official `flick/js` component:** `flick/js@0.2.0` — SHA-256 `366d0f48e06663af9fa3358de687ef6849b8e7cfce2d5f826ba2050bcd71e350` (4,223,041 bytes)
- **Official `flick/go` component:** `flick/go@0.2.0` — SHA-256 `70d978b590272fcd717610d83c7a3e80b9374c4822822c910cb5a64be9478f2e` (7,674,081 bytes)
- **Cache state:**
  - `FLICK_WASMTIME_CACHE_DIR`: isolated per benchmark run, warmed during scenario warmup (measuring warm Wasmtime disk-cache loads, not cold Cranelift JIT compilation).
  - `XDG_CACHE_HOME`: isolated per benchmark run, warmed during scenario warmup.
  - **Hit:** unchanged project build.
  - **Miss:** unique comment appended to `src/Mtl/StateT.fl` (`examples/mtl`) or `src/Main.fl` (`small`), forcing a build-cache miss while keeping unchanged modules cached.

## Baseline measurements (before host CBOR-budget & cache-key optimization)

### Uninstrumented process wall time & peak RSS (5 fresh processes, median (min–max))

| Scenario | Hit Wall (ms) | Hit Peak RSS (MiB) | Miss Wall (ms) | Miss Peak RSS (MiB) | Miss − Hit (ms) |
| --- | ---: | ---: | ---: | ---: | ---: |
| `mtl / JS RT` | 285.0 (280.0–285.4) | 47.5 (47.3–47.8) | 781.7 (770.4–794.6) | 185.6 (184.8–186.0) | +496.6 |
| `mtl / JS Effect` | 289.0 (286.7–289.5) | 47.2 (46.8–47.3) | 1464.1 (791.3–1582.6)* | 183.6 (183.4–185.8) | +1175.1 |
| `small / JS RT` | 448.5 (444.7–477.2) | 47.3 (47.1–47.3) | 960.1 (899.9–1048.9) | 155.9 (155.8–156.5) | +511.5 |
| `small / JS Effect` | 341.4 (312.3–351.5) | 47.3 (47.0–47.5) | 831.0 (770.2–874.3) | 155.9 (155.8–156.0) | +489.6 |
| `small / Go control` | 76.4 (75.2–83.4) | 66.5 (66.5–66.8) | 825.7 (803.3–885.7) | 195.8 (193.0–196.5) | +749.2 |

*\*Transient host load spike during the baseline `mtl / JS Effect` samples; min was 791.3 ms.*

### Baseline miss sub-stage attribution (`--timings --diagnostics json`, median ms)

| Stage / Sub-stage | `mtl / JS RT` | `small / JS RT` | `small / JS Effect` | `small / Go control` |
| --- | ---: | ---: | ---: | ---: |
| `program_bytes` (bytes) | 3,755,147 | 3,553,256 | 3,553,260 | 3,553,236 |
| `inputs_bytes` (bytes) | 16,130,371 | 15,731,470 | 15,730,974 | 15,739,710 |
| **[stage] Emitting backend output** | **412.4** | **499.2** | **403.6** | **717.7** |
| `backend component init (wasmtime load)` | 26.0 | 30.6 | 29.6 | 34.6 |
| `backend component tools discovery` | 43.0 | 49.2 | 46.2 | 11.7 |
| `backend prepare request` | 107.8 | 119.7 | 114.5 | 92.2 |
| `backend handshake` (`validate_size`) | **17.1** | **29.5** | **17.1** | **16.8** |
| `backend host input snapshot` | 7.6 | 8.9 | 8.2 | 8.1 |
| `backend host request validation (decode #1)` | **45.0** | **67.0** | **48.0** | **47.2** |
| `backend check-target (guest decode #2)` | 59.9 | 84.9 | 64.7 | 56.2 |
| `backend generate #1 (guest decode #3)` | 77.3 | 76.8 | 67.3 | 61.7 |
| `backend result validation #1` | **1.2** | **0.8** | **0.5** | **21.5** |
| `backend determinism replay generate #2 (guest decode #4)` | 76.6 | 75.8 | 62.7 | 61.6 |
| `backend replay validation #2` | **1.2** | **0.6** | **0.6** | **20.7** |
| `backend cache key` | **17.5** | **26.7** | **17.9** | **20.0** |
| `backend external tool steps` | 0.0 | 0.0 | 0.0 | 289.6 |
| `backend emission residual` | 0.3 | 0.4 | 0.3 | 0.8 |
| `atomic output publication` | 0.3 | 0.2 | 0.1 | 2.9 |

## Actionable finding implemented (zero-risk host optimization)

Attribution immediately revealed a pure host serialization waste in
`flick-backend`:

- `validate_size(request, limits)` (called once in `handshake` and a second time
  inside `validate_request`), `validate_result` (called twice: first result +
  determinism replay), and `cache_key(request)` serialized `request.program: Vec<u8>`
  (3.75 MB) and `result.artifacts[*].bytes: Vec<u8>` via `ciborium::into_writer`
  using `serde`'s default `Vec<u8>` integer-array encoding — emitting 3.75M–10M
  individual CBOR integers per call and allocating a ~7.5 MB temporary buffer
  for `cache_key`.
- **Fix (in `flick-backend/src/{request,validate,lib}.rs`):**
  - Account for `request.program.len()` and `artifact.bytes.len()` directly in
    `O(1)` against `Limits::{request_bytes, result_bytes}` while still
    CBOR-budgeting every metadata field via `Budget`.
  - Stream `cache_key(request)` directly into `sha2::Sha256` without allocating
    a temporary buffer, hashing the CBOR-encoded metadata followed by
    `program.len()` and `&request.program` in one slice update.
- **Guarantees preserved:** Identical component bytes (`just check-component js`
  and `just check-component go` unchanged), identical request/result size limit
  enforcement, full host `validate_request` (decode #1), `check-target` (decode
  #2), determinism replay `generate #2` (decode #4 / `Failure::Unstable`
  detection), external tool authorization, and build-cache invalidation.

## Matched after-optimization measurements

### Uninstrumented process wall time & peak RSS (5 fresh processes, median (min–max))

| Scenario | Hit Wall (ms) | Hit Peak RSS (MiB) | Miss Wall (ms) | Miss Peak RSS (MiB) | Miss − Hit (ms) |
| --- | ---: | ---: | ---: | ---: | ---: |
| `mtl / JS RT` | 281.2 (278.0–291.8) | 47.5 (47.0–47.5) | 697.1 (690.1–736.1) | 185.1 (184.8–185.6) | +415.9 |
| `mtl / JS Effect` | 286.1 (282.0–288.0) | 47.3 (46.8–47.5) | 685.1 (673.3–692.8) | 185.1 (184.3–185.6) | +399.0 |
| `small / JS RT` | 290.8 (285.3–294.9) | 47.0 (46.9–47.5) | 582.1 (572.0–592.0) | 155.4 (155.1–155.6) | +291.3 |
| `small / JS Effect` | 288.6 (280.4–292.7) | 47.3 (46.8–47.5) | 584.9 (577.9–607.6) | 155.3 (155.2–155.9) | +296.3 |
| `small / Go control` | 75.1 (73.5–77.3) | 66.8 (66.3–67.0) | 649.8 (629.3–663.7) | 195.3 (193.5–196.5) | +574.7 |

### After-optimization miss sub-stage attribution (`--timings --diagnostics json`, median (min–max) ms)

| Stage / Sub-stage | `mtl / JS RT` | `mtl / JS Effect` | `small / JS RT` | `small / JS Effect` | `small / Go control` |
| --- | ---: | ---: | ---: | ---: | ---: |
| `program_bytes` (bytes) | 3,755,147 | 3,755,147 | 3,553,256 | 3,553,260 | 3,553,236 |
| `inputs_bytes` (bytes) | 16,130,371 | 16,129,875 | 15,731,470 | 15,730,974 | 15,739,710 |
| **[stage] Starting** | 1.3 | 1.3 | 1.3 | 1.3 | 1.2 |
| **[stage] Resolving project** | 177.8 | 177.2 | 173.5 | 175.7 | 50.6 |
| **[stage] Checking build cache** | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| **[stage] Preparing compiler** | 6.4 | 6.0 | 6.0 | 6.4 | 5.9 |
| **[stage] Compiling Flick modules** | 68.9 | 69.7 | 19.1 | 18.8 | 17.1 |
| **[stage] Emitting backend output** | **323.7** (313.5–328.1) | **313.0** (308.5–316.0) | **263.8** (259.3–266.5) | **266.8** (257.9–275.4) | **516.9** (492.8–551.6) |
| **[stage] Writing artifacts** | 0.3 | 0.3 | 0.2 | 0.1 | 3.2 |
| **[stage] Saving build cache** | 114.5 | 109.1 | 103.0 | 105.1 | 14.3 |
| `backend component init (wasmtime load)` | 25.6 (24.8–26.9) | 26.0 (24.6–26.7) | 25.9 (24.8–26.7) | 25.8 (24.6–27.0) | 32.8 (31.9–33.3) |
| `backend component tools discovery` | 42.9 (42.1–44.0) | 43.1 (43.0–45.0) | 42.4 (42.2–42.7) | 42.7 (42.4–44.1) | 11.2 (10.9–12.2) |
| `backend preflight & descriptor` | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| `backend prepare request` | 111.1 (108.0–113.8) | 111.2 (109.5–114.6) | 97.2 (94.5–99.2) | 97.8 (96.5–102.9) | 85.4 (83.1–88.4) |
| ↳ `prepare: preflight & prune` | 37.3 (36.3–38.7) | 37.4 (36.8–38.1) | 34.1 (33.7–35.0) | 34.2 (33.4–35.8) | 32.6 (32.2–33.7) |
| ↳ `prepare: program build` | 9.4 (8.7–9.6) | 8.9 (8.7–9.9) | 7.4 (6.9–8.1) | 7.7 (7.5–8.7) | 8.1 (7.7–8.4) |
| ↳ `prepare: program CBOR encode` | 19.6 (18.3–19.9) | 18.7 (18.1–21.2) | 14.8 (14.6–15.8) | 15.2 (15.1–17.0) | 15.3 (14.9–16.4) |
| ↳ `prepare: projections & inputs` | 41.0 (40.6–41.3) | 41.6 (41.5–43.3) | 36.9 (35.9–38.6) | 37.2 (36.5–37.9) | 25.3 (25.2–26.3) |
| `backend runtime dependencies` | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| `backend handshake` | **0.4** (0.4–0.6) | **0.5** (0.4–0.5) | **0.3** (0.3–0.3) | **0.4** (0.3–0.4) | **0.3** (0.3–0.3) |
| `backend host input snapshot` | 7.6 (7.5–7.8) | 7.5 (7.4–7.7) | 7.3 (7.1–7.3) | 7.4 (7.3–7.6) | 7.7 (7.5–7.8) |
| `backend host request validation (decode #1)` | **29.3** (27.9–31.5) | **30.5** (29.0–30.7) | **26.2** (25.7–27.1) | **26.5** (26.0–29.2) | **28.1** (26.7–30.9) |
| `backend check-target (guest decode #2)` | 45.4 (44.1–46.4) | 43.6 (42.4–46.9) | 40.6 (40.0–41.4) | 41.2 (38.8–42.3) | 36.5 (35.8–39.3) |
| `backend generate #1 (guest decode #3)` | 63.6 (59.9–63.8) | 57.6 (55.5–57.8) | 43.7 (43.6–44.9) | 44.3 (42.0–46.2) | 41.5 (41.0–44.0) |
| `backend result validation #1` | **0.3** (0.3–0.4) | **0.3** (0.3–0.3) | **0.2** (0.2–0.3) | **0.3** (0.2–0.3) | **0.9** (0.9–1.0) |
| `backend determinism replay generate #2 (guest decode #4)` | 61.2 (59.5–64.8) | 57.5 (55.5–58.0) | 43.9 (43.3–44.8) | 44.8 (41.9–45.9) | 42.5 (40.3–43.3) |
| `backend replay validation #2` | **0.3** (0.3–0.4) | **0.3** (0.3–0.3) | **0.2** (0.2–0.3) | **0.3** (0.3–0.3) | **1.0** (1.0–1.3) |
| `backend cache key` | **1.9** (1.9–1.9) | **1.9** (1.9–2.0) | **1.8** (1.8–1.8) | **1.8** (1.8–2.0) | **1.8** (1.8–1.8) |
| `backend external tool steps` | 0.0 | 0.0 | 0.0 | 0.0 | 268.4 (247.5–291.7) |
| `backend emission residual` | 0.3 (0.3–0.3) | 0.3 (0.3–0.3) | 0.3 (0.2–0.3) | 0.3 (0.2–0.3) | 0.6 (0.6–0.8) |
| `atomic output publication` | 0.3 (0.3–0.3) | 0.3 (0.3–0.3) | 0.2 (0.1–0.2) | 0.1 (0.1–0.2) | 3.2 (3.0–3.3) |

### Direct before/after delta on targeted host stages

| Stage | `mtl / JS RT` | `small / Go control` |
| --- | ---: | ---: |
| `backend handshake` (`validate_size`) | 17.1 → **0.4 ms** (−16.7 ms, 43× faster) | 16.8 → **0.3 ms** (−16.5 ms, 56× faster) |
| `backend host request validation` | 45.0 → **29.3 ms** (−15.7 ms) | 47.2 → **28.1 ms** (−19.1 ms) |
| `backend result validation #1` | 1.2 → **0.3 ms** (−0.9 ms) | 21.5 → **0.9 ms** (−20.6 ms, 24× faster) |
| `backend replay validation #2` | 1.2 → **0.3 ms** (−0.9 ms) | 20.7 → **1.0 ms** (−19.7 ms, 21× faster) |
| `backend cache key` | 17.5 → **1.9 ms** (−15.6 ms, 9.2× faster) | 20.0 → **1.8 ms** (−18.2 ms, 11× faster) |
| **Total direct host-stage savings** | **−49.8 ms** | **−94.1 ms** |

## Remaining dominant contributors and deferral decision

After removing the host byte-by-byte CBOR overhead, `backend emission residual`
is `0.3 ms` (<0.1% of emission), and the remaining cost on a `mtl / JS RT`
source miss (`697.1 ms` total, `323.7 ms` in `Emitting backend output`) is
concentrated in three areas:

1. **4× `Program` CBOR decode + 3× fresh guest WASM calls (`199.5 ms`, 61.6% of emission):**
   - `request_validation` (decode #1): `29.3 ms`
   - `check-target` (guest decode #2): `45.4 ms`
   - `generate #1` (guest decode #3): `63.6 ms`
   - `determinism replay generate #2` (guest decode #4): `61.2 ms`
   - The dominant driver is `program_bytes = 3.55–3.75 MB` even for a 7-line
     standalone program, because every `BuildRequest` ships all 80+ built-in
     stdlib IR modules.
2. **`backend prepare request` (`111.1 ms`, 34.3% of emission):**
   - `preflight & prune` (`37.3 ms`), `projections & inputs` (`41.0 ms`, writing
     and hashing `16.1 MB` of declared inputs dominated by ~15 MB of built-in
     `.iface` JSON), and `program CBOR encode` (`19.6 ms`).
3. **Outside emission (`Resolving project` `177.8 ms`, `Saving build cache` `114.5 ms`):**
   - On JS builds, project/runtime package inspection (`packages::report` +
     `node_modules` tree hashing) runs both during initial key computation and
     again while saving the build cache, alongside `WasmBackend::load`
     (`25.6 ms`) and Node tool discovery (`42.9 ms`). On Go (`small / Go`),
     `Resolving project` is `50.6 ms` and `Saving build cache` is `14.3 ms`,
     while `go build` (`backend external tool steps`) takes `268.4 ms`.

**Decision on the trust / validation / determinism-replay path:**
We explicitly **defer** modifying the host trust boundary, skipping
`validate_request` (decode #1), skipping `check-target` (decode #2), skipping
determinism replay `generate #2` (decode #4), or dropping built-in `.iface`
inputs from `BuildRequest`. All four decodes and the fresh-store-per-call
boundary are load-bearing parts of the `flick:backend@0.1.0` security and
determinism contract (rejecting malformed IR, unapproved capabilities, stateful
guests, and nondeterministic outputs before caching). The right future lever to
cut the remaining ~200 ms decode cost without weakening any guarantee is
reducing `program_bytes` and `inputs_bytes` (for example, reachability-scoped
built-in IR/interface delivery coordinated with `--declarations` and external
backend consumers) and caching repeated runtime-graph hashing within a single
CLI invocation.
