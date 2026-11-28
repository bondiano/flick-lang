# Crate Split Evaluation & Rebuild Evidence (XD3)

This document records the repeatable rebuild, coupling, API-surface, and browser
bundle measurements used to evaluate crate extractions out of `flick-core` after
capability and layering cleanup (issue #406, parent #374). Crate extraction is
conditional on measured build or boundary benefit: moving files into additional
crates is not itself a goal.

## Reproducing these measurements

All rebuild timings were collected with incremental compilation disabled
(`CARGO_INCREMENTAL=0`) and raw `rustc` (bypassing any compiler cache such as
`kache`) against a warmed `target/` directory, reporting the median and min–max
across 3 runs per scenario plus Cargo `-vv` dirty-unit reasons:

```bash
export CARGO_INCREMENTAL=0
export RUSTC_WRAPPER=""
export FLICK_STD_PATH="$PWD/packages/std"

# Warm no-op baselines
cargo check -p flick-diagnostics
cargo check -p flick-syntax
cargo check -p flick-ffi
cargo check -p flick-core --no-default-features
cargo check -p flick-core --no-default-features --features embedded-std,ide
cargo check -p flick-wasm --target wasm32-unknown-unknown
cargo build -p flick

# Single-file edit protocol (repeat 3x, append a unique comment, time cargo -vv, restore file + warm)
# Browser compiler & tracked program sizes (with Binaryen 133 on PATH):
just wasm
just bundle-size
```

Using `CARGO_INCREMENTAL=0` measures full non-incremental per-crate
recompilation when Cargo marks a crate dirty — an upper bound on the compile
time a crate split could save on single-file edits, without incremental-cache
noise.

### Pinned environment

- **Host OS / CPU / RAM:** Linux `6.6.87.2-microsoft-standard-WSL2` x86_64 — AMD Ryzen 9 9950X3D 16-Core Processor (32 threads, 47 GiB RAM)
- **Toolchain:** `rustc 1.95.0 (59807616e 2026-04-14)` | Binaryen `wasm-opt` 133
- **Base commit:** `b5f8d23ee` (`main`)

## Workspace crate graph & `flick-core` coupling audit

Three leaf crates from the original split candidate set were already extracted
in `#728` (`fd9f6fd1e`):

| Crate / module | Files | LOC | Public items (`pub`) | Direct workspace `[dependencies]` consumers |
| --- | ---: | ---: | ---: | --- |
| `flick-diagnostics` (extracted) | 12 | 1,327 | 45 | `flick-syntax`, `flick-ffi`, `flick-core`, `flick-backend-js`, `flick-backend-go` |
| `flick-syntax` (extracted) | 60 | 11,423 | 113 | `flick-ffi`, `flick-core` |
| `flick-ffi` (extracted) | 8 | 2,054 | 17 | `flick-core`, `flick-backend-js`, `flick-backend-go` |
| `flick-core` (total) | 1,077 | 274,922 | — | `flick`, `flick-wasm`, `flick-model`, `flick-vm` |
| ↳ `flick-core/src/types/` (L2) | 303 | 79,464 | — | internal (`ir`, `opt`, `project`, `build`, `ide`, …) |
| ↳ `flick-core/src/opt/` (L3) | 293 | 77,304 | — | internal (`project`, `build`, …) |
| ↳ `flick-core/src/project/` (L4) | 128 | 33,603 | — | internal (`build`, `ide`, `projection`, `lib::{check,prove}`) |
| ↳ `flick-core/src/build/` (L5) | 12 | 2,901 | — | leaf inside `flick-core` |
| ↳ `flick-core/src/ide/` (L6) | 90 | 19,852 | — | leaf inside `flick-core` |

`flick-core/src/layer_tests.rs` enforces a strict 7-layer DAG (L0–L6) across
every top-level module in `flick-core`.

### Non-test `#[cfg(feature = ...)]` density in `flick-core/src/`

| Feature | Total sites | Breakdown by top-level module |
| --- | ---: | --- |
| `native-project` | 325 | `project`: 248, `types`: 31, `build`: 25, `doc`: 7, `plugin`: 6, `codegen`: 2, `manifest`: 2, `opt`: 2, `iface`: 1, `ir`: 1 |
| `ide` | 75 | `types`: 46, `project`: 26, `lib`: 2, `resolve`: 1 (`ide/`: **0**) |
| `native-js` | 71 | `build`: 40, `codegen`: 16, `project`: 10, `opt`: 3, `numeric`: 2 |
| `contracts` | 24 | `project`: 14, `types`: 6, `ir`: 3, `logic`: 1 |
| `commands` | 6 | `plugin`: 6 |
| `embedded-std` | 3 | `types`: 3 |

### Cross-module coupling of remaining candidates

- **`project/` (Layer 4):** Not a leaf — depended on internally by `build`,
  `ide`, `projection`, and `flick_core::{check, prove}` in `lib.rs`. Makes 289
  non-test `crate::` references across 15 lower modules (`source`, `plugin`,
  `syntax`, `diagnostics`, `types`, `resolve`, `iface`, `ir`, `effects`,
  `clock`, `codegen`, `manifest`, `implicit_packages`, `VERSION`) and imports 6
  `pub(crate)` items from lower modules (`iface::closed`,
  `plugin::{net_endpoint, placed, token_kind}`,
  `resolve::{shortest_cycle, syntax_references}`). Extracting `project/` still
  leaves 77 `native-project` gates inside `flick-core`.
- **`ide/` + `projection.rs` (Layer 6 / L5):** Zero internal dependents inside
  `flick-core`, but makes 283 non-test `crate::` references across 13 modules
  (`syntax`, `types`, `source`, `iface`, `project`, `resolve`, `diagnostics`,
  `doc`, `logic`, `plugin`, `codegen`, `fmt`) and imports 3 `pub(crate)` items
  (`resolve::item_removal`, `types::catch_to_try_text`,
  `types::describe_targets`). Extracting `ide/` removes only 2 of the 75
  `#[cfg(feature = "ide")]` sites (`lib.rs`); 73 remain in `types/` (46),
  `project/` (26), and `resolve/` (1).

## Baseline rebuild measurements (before `build.rs` fix)

### Warm no-op builds (median of 3 runs, 0 dirty units)

| Command | Median (s) | Min–Max (s) |
| --- | ---: | ---: |
| `cargo check -p flick-diagnostics` | 0.092 | 0.091–0.094 |
| `cargo check -p flick-syntax` | 0.094 | 0.094–0.095 |
| `cargo check -p flick-ffi` | 0.096 | 0.095–0.097 |
| `cargo check -p flick-core --no-default-features` | 0.099 | 0.098–0.134 |
| `cargo check -p flick-core --no-default-features --features embedded-std,ide` | 0.105 | 0.102–0.109 |
| `cargo check -p flick-wasm --target wasm32-unknown-unknown` | 0.113 | 0.109–0.116 |
| `cargo build -p flick` | 0.133 | 0.128–0.139 |

### Single-file edit rebuilds (median of 3 uncached runs, `CARGO_INCREMENTAL=0`)

| Edited file | Target command | Median (s) | Min–Max (s) | Dirty workspace crates |
| --- | --- | ---: | ---: | --- |
| `flick-diagnostics/src/diagnostics/suggest.rs` | `cargo check -p flick-diagnostics` | **0.190** | 0.189–0.201 | `flick-diagnostics` (1) |
| `flick-diagnostics/src/diagnostics/suggest.rs` | `cargo build -p flick` | 27.965 | 27.797–28.100 | `flick-diagnostics` → … → `flick` (7) |
| `flick-diagnostics/src/diagnostics/codes.rs` | `cargo build -p flick` | 28.764 | 28.293–28.771 | Same 7 + **`flick-core` `build.rs` rebuilt** |
| `flick-syntax/src/syntax/parser/accessor.rs` | `cargo check -p flick-syntax` | **0.341** | 0.341–0.355 | `flick-syntax` (1) |
| `flick-syntax/src/syntax/parser/accessor.rs` | `cargo build -p flick` | 27.841 | 27.704–27.876 | `flick-syntax` → … → `flick` (6) |
| `flick-syntax/src/syntax/lexer/mod.rs` | `cargo build -p flick` | 28.243 | 28.159–28.500 | Same 6 + **`flick-core` `build.rs` rebuilt** |
| `flick-ffi/src/ffi/check.rs` | `cargo check -p flick-ffi` | **0.309** | 0.299–0.314 | `flick-ffi` (1) |
| `flick-ffi/src/ffi/check.rs` | `cargo check -p flick-backend-js --features component` | **1.000** | 0.919–1.078 | `flick-ffi`, `flick-backend-js` (2; no `flick-core`) |
| `flick-ffi/src/ffi/check.rs` | `cargo build -p flick` | 27.750 | 27.695–27.789 | `flick-ffi` → … → `flick` (5) |
| `runtime/src/rt/builtins.ts` | `cargo build -p flick` | **0.119** | 0.119–0.122 | None (0) |
| `runtime/rt_exports.txt` | `cargo build -p flick` | 29.268 | 29.162–29.332 | `flick-backend-support` → … → `flick` (6) |
| `runtime-go/flickrt/adt_ops.go` | `cargo build -p flick` | 27.201 | 27.001–27.592 | `flick-backend-go` → `flick-core` → `flick-vm` → `flick` (4) |
| `runtime-go/flickrt/adt_ops.go` | `cargo check -p flick-wasm --target wasm32-unknown-unknown` | **0.103** | 0.101–0.104 | None (0) |
| `packages/core/src/Option.fl` (code) | `cargo build -p flick` | 26.014 | 25.985–26.042 | `flick-core`, `flick-vm`, `flick` (3) |
| `packages/core/src/Option.fl` (code) | `cargo check -p flick-core --no-default-features` | 8.725 | 8.642–8.848 | `flick-core` (1) |
| `packages/core/src/Option.fl` (code) | `cargo check -p flick-wasm --target wasm32-unknown-unknown` | 9.078 | 9.029–9.085 | `flick-core`, `flick-wasm` (2) |
| `packages/core/src/Option.fl` (doc `///`) | `cargo check -p flick-core --no-default-features` | 8.688 | 8.618–8.752 | `flick-core` (1) |
| `packages/core/src/Option.fl` (doc `///`) | `cargo check -p flick-wasm --target wasm32-unknown-unknown` | 8.923 | 8.851–8.971 | `flick-core`, `flick-wasm` (2) |
| `packages/std/src/Duration.fl` | `cargo build -p flick` (`embedded-std` OFF) | **0.131** | 0.127–0.134 | None (0) |
| `packages/std/src/Duration.fl` | `cargo check -p flick-core --no-default-features` (`embedded-std` OFF) | **8.653** | 8.625–8.670 | **`flick-core` (1 — spurious!)** |
| `packages/std/src/Duration.fl` | `cargo check -p flick-wasm --target wasm32-unknown-unknown` (`embedded-std` ON) | 9.084 | 8.968–9.104 | `flick-core`, `flick-wasm` (2) |
| `packages/std/src/modules.txt` | `cargo build -p flick` (`embedded-std` OFF) | **25.611** | 25.376–31.828 | **`flick-core`, `flick-vm`, `flick` (3 — spurious!)** |
| `packages/std/src/modules.txt` | `cargo check -p flick-core --no-default-features` (`embedded-std` OFF) | **8.524** | 8.466–8.611 | **`flick-core` (1 — spurious!)** |
| `packages/std/src/go/time.txt` | `cargo build -p flick` (`embedded-std` OFF) | **25.773** | 25.675–26.037 | **`flick-core`, `flick-vm`, `flick` (3 — spurious!)** |
| `packages/ui/src` (file add/remove) | `cargo build -p flick` (`embedded-std` OFF) | **25.721** | 25.476–26.039 | **`flick-core`, `flick-vm`, `flick` (3 — spurious!)** |
| `flick-core/src/project/config.rs` | `cargo check -p flick-core --no-default-features` | 8.656 | 8.531–8.818 | `flick-core` (1) |
| `flick-core/src/project/config.rs` | `cargo build -p flick` | 25.752 | 25.684–26.106 | `flick-core`, `flick-vm`, `flick` (3) |
| `flick-core/src/ide/hover.rs` | `cargo check -p flick-core --no-default-features` (`ide` OFF) | **0.098** | 0.096–0.098 | None (0) |
| `flick-core/src/ide/hover.rs` | `cargo check -p flick-wasm --target wasm32-unknown-unknown` (`ide` ON) | 9.054 | 8.995–9.150 | `flick-core`, `flick-wasm` (2) |
| `flick-core/src/ide/hover.rs` | `cargo build -p flick` (`ide` ON) | 25.765 | 25.686–26.221 | `flick-core`, `flick-vm`, `flick` (3) |

## Build-script audit & adopted `flick-core/build.rs` narrowing

Auditing all three workspace build scripts (`flick-core/build.rs`,
`flick-backend-support/build.rs`, and `backends/go/build.rs`):

- `flick-backend-support/build.rs` watches only 7 explicit policy/manifest files
  (`src/kernel/manifest.txt`, `runtime/rt_{exports,stubs,unavailable}.txt`,
  `packages/effect-ts/{rt_bridge_stubs.txt,compiler-bindings.json,compiler-families.json}`).
  Edits to `runtime/src/rt/*.ts` trigger 0 Rust rebuilds (`0.119s`). Clean.
- `backends/go/build.rs` embeds `runtime-go/` (excluding `*_test.go`) and is
  already isolated behind optional `flick-backend-go` (`native-project` only;
  `flick-wasm` takes `0.103s` / 0 rebuilds on Go runtime edits). Clean.
- **`flick-core/build.rs` had four concrete over-triggering defects:**
  1. `implicit_sources` ran unconditionally even when `embedded-std` was OFF
     (`builtin_sources.rs` only `include!`s `OUT_DIR/implicit_sources.rs` under
     `#[cfg(feature = "embedded-std")]`). That emitted `cargo:rerun-if-changed`
     for all 9 implicit packages' `src/modules.txt`, `src/go/*.txt`, and — for
     packages lacking `src/go/` (`ui`, `js`) — the entire `packages/<pkg>/src`
     directory, causing 25.6–25.8s spurious `flick` rebuilds.
  2. When `!native-project && !embedded-std` (minimal `flick-core`, `flick-vm`),
     `strip_tree` walked and watched all 9 implicit packages (222 `.fl` files,
     991 KB) in addition to `packages/core` (26 `.fl` files, 138 KB), causing
     8.5–8.7s spurious minimal `flick-core` rebuilds on `packages/std` edits.
  3. `build.rs` unconditionally compiled `#[path]` copies of
     `flick-diagnostics` (`codes.rs`, `model.rs`, `source.rs`), `flick-syntax`
     (`kind.rs`, `lexer/mod.rs`), and `src/builtin_source.rs` even when
     `native-project` was ON (where doc-stripping is never used).
  4. `build.rs` used unconditional `fs::write`, rewriting unchanged stripped
     files and `implicit_sources.rs`.

### Adopted fix (`flick-core/build.rs`)

1. Gate doc-stripping modules (`codes`, `model`, `diagnostics`, `kind`, `lexer`,
   `source`, `syntax`, `builtin_source`, `strip_tree`) behind
   `#[cfg(not(feature = "native-project"))]`.
2. Gate `implicit_sources`, `package_modules`, `package_go_tables`, and the
   implicit-package loop in `strip_tree` behind `#[cfg(feature = "embedded-std")]`
   (and remove stale `OUT_DIR/implicit_sources.rs` and `OUT_DIR/<implicit_pkg>`
   when `embedded-std` is OFF so incremental upgrades stay clean without
   `cargo clean`).
3. Write generated/stripped outputs through `write_if_changed` to preserve
   output mtimes when bytes are unchanged.
4. Pin the 2×2 `(native-project × embedded-std)` output invariant in
   `flick-core/src/types/builtins_tests.rs`
   (`build_script_generates_only_feature_required_outputs`).

### Before / after rebuild deltas (`flick-core/build.rs` narrowing)

| Scenario | Before (median) | After (median) | Delta |
| --- | ---: | ---: | --- |
| `packages/std/src/Duration.fl` → `cargo check -p flick-core --no-default-features` (`embedded-std` OFF) | 8.653s (1 crate) | **0.098s (0 crates)** | **−8.555s (88× faster)** |
| `packages/std/src/modules.txt` → `cargo check -p flick-core --no-default-features` (`embedded-std` OFF) | 8.524s (1 crate) | **0.101s (0 crates)** | **−8.423s (84× faster)** |
| `packages/std/src/modules.txt` → `cargo build -p flick` (`embedded-std` OFF) | 25.611s (3 crates) | **0.129s (0 crates)** | **−25.482s (199× faster)** |
| `packages/std/src/go/time.txt` → `cargo build -p flick` (`embedded-std` OFF) | 25.773s (3 crates) | **0.127s (0 crates)** | **−25.646s (203× faster)** |
| `packages/ui/src` dir touch → `cargo build -p flick` (`embedded-std` OFF) | 25.721s (3 crates) | **0.123s (0 crates)** | **−25.598s (209× faster)** |
| `flick-diagnostics/src/diagnostics/codes.rs` → `cargo build -p flick` | 28.764s (`build.rs` dirty) | **27.932s (`build.rs` untouched)** | **−0.832s** |
| `flick-syntax/src/syntax/lexer/mod.rs` → `cargo build -p flick` | 28.243s (`build.rs` dirty) | **27.946s (`build.rs` untouched)** | **−0.297s** |
| `packages/core/src/Option.fl` (code) → `cargo build -p flick` | 26.014s (3 crates) | 25.524s (3 crates) | Preserved (no under-invalidation) |
| `packages/core/src/Option.fl` (code) → `cargo check -p flick-wasm` | 9.078s (2 crates) | 8.980s (2 crates) | Preserved (no under-invalidation) |
| `packages/std/src/Duration.fl` → `cargo check -p flick-wasm` (`embedded-std` ON) | 9.084s (2 crates) | 8.935s (2 crates) | Preserved (no under-invalidation) |

*Note on doc-only edits to `packages/core/src/Option.fl` under `!native-project`:*
`write_if_changed` preserves the `OUT_DIR/core/Option.fl` mtime (so caching
wrappers or incremental dep-info avoid redundant work), but Cargo itself still
re-invokes `rustc` on `flick-core` whenever `build.rs` re-runs (`8.477s` minimal
check after vs `8.688s` before under `CARGO_INCREMENTAL=0`).

## Browser compiler (`flick-wasm`) and bundle sizes

Measured via `just wasm` (Binaryen 133 `wasm-opt`, `opt-level = "z"`,
`lto = "fat"`, `strip = "debuginfo"`) and `just bundle-size`:

- **`flick_wasm.wasm`:** `7,858,330` bytes pre-dedup → **`4,890,669` bytes**
  post-dedup (`1,871,465` bytes gzipped) vs **`4,893,000`-byte** budget
  (**2,331 bytes** of headroom). Identical byte size after `build.rs` narrowing.
- **Tracked program bundles (`just bundle-size`, all 13 within budget):**
  `pure` `12.5/13 KB`, `pure-rt` `5.3/6 KB`, `counter` `116.6/118 KB`,
  `counter-rt` `80.0/80 KB`, `playground` `129.9/150 KB`,
  `playground-rt` `92.7/150 KB`, `atoms` `116.4/117 KB`, `atoms-rt` `79.8/80 KB`,
  `rpc` `55.5/56 KB`, `rpc-rt` `7.9/8 KB`, `email` `47.7/48 KB`,
  `email-rt` `11.4/12 KB`, `fullstack` `55.8/56 KB`.

## Decision table

| Candidate | Rebuild benefit | Removed coupling | Added API surface | Browser size (`flick_wasm.wasm`) | Disposition |
| --- | --- | --- | --- | --- | --- |
| **`flick-diagnostics`** (12 files, 1.3K LOC) | **High for leaf work:** `0.190s` local check vs `27.965s` CLI build (`147×`); CLI rebuild still runs downstream chain. | `flick-syntax`, `flick-ffi`, `backends/js`, and `backends/go` depend on diagnostics without `flick-core`. | 45 `pub` items (`source`, `diagnostics`); re-exported by `flick-core`. | Included in baseline (`4,890,669 B`). | **ADOPTED** (prior, `#728`) |
| **`flick-syntax`** (60 files, 11.4K LOC) | **High for parser work:** `0.341s` local check vs `27.841s` CLI build (`82×`). | `flick-ffi` parses `.fl` signatures without `flick-core`; isolates lossless CST/lexer/parser. | 113 `pub` items (`syntax`); `flick-core::syntax` adds `std_head_catalog()` wrappers. | Included in baseline (`4,890,669 B`). | **ADOPTED** (prior, `#728`) |
| **`flick-ffi`** (8 files, 2.1K LOC) | **High for FFI & JS backend:** `0.309s` local check, `1.000s` `flick-backend-js --features component` check (no `flick-core`) vs `27.750s` CLI build. | `backends/js` and `backends/go` import FFI surface directly without `flick-core`; `toml` stays behind `native-project`. | 17 `pub` items (`ffi`); re-exported by `flick-core`. | Included in baseline (`4,890,669 B`). | **ADOPTED** (prior, `#728`) |
| **`flick-core/build.rs` trigger narrowing** | **84×–209× faster** on spurious std/ui/go-table triggers (`8.5–25.8s` → `0.10–0.13s`, 0 crates); decouples `build.rs` from `diagnostics`/`syntax` in native builds (`−0.3s` to `−0.8s`). | Removes `packages/{std,…,js}` watch coupling when `embedded-std` is OFF and removes `diagnostics`/`syntax` build-script coupling when `native-project` is ON. | **None** (zero new crate, zero public API change). | **0 B** change (`4,890,669 B`, within `4,893,000 B` budget). | **ADOPTED** (`#406`) |
| **Embedded std/core data crate** (e.g. `flick-builtin-sources`) | **None for `flick-core` consumers:** empirical control (`flick-backend-go` on `runtime-go/flickrt/adt_ops.go` = `27.201s`; `flick-ffi` → `flick-backend-js` = `1.000s`) proves Cargo still marks downstream `flick-core` dirty when a leaf data crate rebuilds. After `build.rs` narrowing, `embedded-std=OFF` already skips implicit packages completely. | Moves `flick-ir`/`fln-literals`/`serde` build-deps off `flick-core` only; no non-`flick-core` consumer needs embedded Flick sources. | New workspace crate exposing `CORE`, `IMPLICIT`, `IMPLICIT_PACKAGES`, `without_docs`, plus relocating `builtins_tests` inventory checks. | Risk of eroding the `2,331 B` raw headroom if cross-crate boundaries inhibit deduplication. | **DEFERRED** |
| **`project` crate** (`flick-core/src/project/`, 128 files, 33.6K LOC) | **Negligible:** `project` is Layer 4; `flick` always links it, and edits to `types`/`ir`/`opt` (60% of `flick-core`) still rebuild `project`. Minimal consumers already gate out 248 `native-project` sites via Cargo features. | Not a leaf: `build`, `ide`, `projection`, and `flick_core::{check,prove}` depend on it; extracting it forces L5–L6 out too and still leaves 77 `native-project` gates in `flick-core`. | Stabilizes 289 `crate::` references across 15 modules and promotes 6 `pub(crate)` items (`iface::closed`, `plugin::{net_endpoint,placed,token_kind}`, `resolve::{shortest_cycle,syntax_references}`) to `pub`. | Unfavorable against `2,331 B` raw headroom; `flick-wasm` still needs in-memory `project`. | **DEFERRED** |
| **`ide` crate** (`flick-core/src/ide/` + `projection.rs`, 91 files, ~20K LOC) | **Moderate only for isolated `ide/`-only edits** (7.2% of `flick-core` LOC; currently `9.054s` `flick-wasm` check / `25.765s` CLI build). Non-IDE consumers already skip `ide/` completely (`0.098s` / 0 crates on `ide/hover.rs` edit). Edits to `types/` or `project/` still rebuild both crates. | **Does not remove `#[cfg(feature = "ide")]` from `flick-core`:** 73 of 75 gates live in `types/` (46), `project/` (26), and `resolve/` (1); only 2 module gates in `lib.rs` would move. | Stabilizes 283 `crate::` references across 13 `flick-core` modules and promotes 3 `pub(crate)` items (`resolve::item_removal`, `types::{catch_to_try_text,describe_targets}`) to `pub`. | Risk to `2,331 B` raw headroom in `flick-wasm` (which enables `ide`) from cross-crate panic paths/monomorphization. | **DEFERRED** |

## Revisit triggers for deferred candidates

Re-open a deferred split only when new evidence satisfies its trigger:

- **Embedded std/core data crate:** an independent crate outside `flick-core`
  requires embedded Flick std sources without depending on `flick-core`, or
  Cargo stabilizes content-hashed `.rmeta` early cutoff so leaf data-crate
  rebuilds stop dirtying `flick-core`.
- **`project` crate:** a real consumer needs `flick-core`'s checking/IR layers
  without `project::Host` and `flick_core::{check, prove}` is willing to move
  to a higher orchestration crate alongside Layers 5–6.
- **`ide` crate:** `ide/`-only iteration becomes a dominant developer bottleneck
  *and* `flick_wasm.wasm` has comfortable size headroom (or `flick-wasm` stops
  bundling the editor query surface), justifying maintaining a public cross-crate
  contract between `flick-core` and `flick-ide`.
