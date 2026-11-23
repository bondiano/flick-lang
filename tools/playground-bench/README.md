# Playground edit / first-emit measurements

`index.mjs` runs the real `docs/src/playground/compiler.worker.ts` and the
WebAssembly compiler in initialized, isolated Chromium workers. It serves
only the worker and its generated WASM bindings; no Astro build or mocked
compiler is involved. Node strips TypeScript syntax without changing worker
logic. The examples are the shipped `hello`, `classes` and `sales-report`
fixtures. Their hashes and the exact worker and WASM hashes are in the report.

The primary comparison uses ordinary optimized production artifacts. The
optional stage probe below is a separate instrumented build and must not be
used for a production latency or bundle-size claim.

## Build and run

Use the pinned Rust, Node, wasm-bindgen and Binaryen versions described in
`CONTRIBUTING.md` and `rust-toolchain.toml`. After `pnpm install`, install
Chromium with `pnpm -C docs exec playwright install chromium`, or set
`CHROMIUM` to a system Chromium. The existing `just docs-wasm` recipe creates
the default input directory.

```sh
just docs-wasm
mkdir -p target/playground-perf
SAMPLES=20 WARMUP=3 node tools/playground-bench/index.mjs \
  docs/src/playground/wasm target/playground-perf/current.json
```

For a comparison, build the two revisions in isolated worktrees, save each
WASM plus bindings in a separate directory, and preserve the baseline worker
source. Build with identical settings:

```sh
CARGO_PROFILE_RELEASE_OPT_LEVEL=z CARGO_PROFILE_RELEASE_LTO=fat \
  CARGO_PROFILE_RELEASE_STRIP=debuginfo \
  cargo build -p flick-wasm --target wasm32-unknown-unknown --release --locked
node tools/wasm-dedup/index.mjs target/wasm32-unknown-unknown/release/flick_wasm.wasm
wasm-bindgen --target web --out-name flick_wasm --out-dir /tmp/flick-before \
  target/wasm32-unknown-unknown/release/flick_wasm.wasm
cp docs/src/playground/compiler.worker.ts /tmp/flick-before/compiler.worker.ts
```

Run the same harness against both saved artifacts, with no competing builds
or benchmark processes. Repeat in reverse order to expose drift:

```sh
BUILD_REVISION=<baseline-sha> \
  WORKER_SOURCE=/tmp/flick-before/compiler.worker.ts \
  node tools/playground-bench/index.mjs /tmp/flick-before target/playground-perf/before.json
BUILD_REVISION=<candidate-sha> \
  node tools/playground-bench/index.mjs /tmp/flick-after target/playground-perf/after.json
```

`BUILD_REVISION` labels the compiler input revision; `harnessRevision` is the
checkout running the measurements. The report also records the working-tree
status, host, browser version, sample count, warmups, raw observations,
min/p50/p95/max, and raw/gzip WASM sizes. Check the pre-bindgen artifact against
the existing `just wasm` raw-byte budget as well: the report's bytes describe
the final browser payload, which is a different stage of the pipeline.

If a browser cannot run on the host, `ENGINE=node` uses Node worker threads
with the same production worker and WASM. This is a V8/WASM control and worker
behavior test, **not a browser-latency measurement**. Its report explicitly
identifies the engine. `FIXTURES=hello` selects a smaller smoke run.

## Scenarios and correctness

Each scenario starts with a fresh worker. The first `WARMUP` edit iterations
are excluded from latency statistics; their memory growth is included in the
peak. A numbered trailing comment changes the source on every iteration
while keeping program behavior constant.

| Scenario | Untimed preparation per iteration | Timed request |
| --- | --- | --- |
| `diagnostics-only` | None | Check changed text |
| `edit-first-emit` | None | Build changed text in debug |
| `checked-first-emit` | Check the changed text | First debug build of that same text |
| `unchanged-profile` | Debug build of the changed text | Release build of the same text |
| `check-after-direct-run` | Direct debug build | Check a following edit |
| `large-check-small-run` | Check the fixture plus 128 typed declarations | Direct build after removing those declarations |

The normal playground refresh checks before emission, so
`checked-first-emit` is an essential control. A direct worker build dispatch
and a build after a completed diagnostics request are different states. Do
not transfer a gain in `edit-first-emit` to the latter or to every UI Run.

Every request must complete with a matching ID and successful fixture result.
Missing, non-finite or negative durations fail. Worker initialization and
requests have explicit timeouts. Peak WASM linear-memory **capacity** comes
from `memory.buffer.byteLength`; this is a high-water mark, not live heap
occupancy, retained Rust allocations or browser process RSS. It includes
preparatory requests and warmups in the same edit sequence.

Outside timing, the harness sends successful, edited, invalid, fixed, empty
and restored revisions through the actual worker in both profiles, requests
declarations, checks UTF-16 error/hover ranges after a non-BMP prefix, and
rejects stale JS, declarations, file maps or entry points after failure. The
`validation` array hashes the complete generated payload. Compare it exactly
between reports; an equal hash is an output parity check, not a latency claim.
The Rust session regressions also compare direct emission against the
original cold `update` + `emit` path and check lazy editing and cache lifetime.

## Optional stage attribution

The production wrapper observes exported `update`, `prepare_emit`, `emit`
and `emit_declarations` calls. Interpret them according to their paths:

- Baseline `emit` includes cold frontend analysis, lowering, backend work and
  output serialization.
- Candidate `prepare_emit` materializes frontend analysis and lowering.
  A call for an already prepared revision has neither operation.
- Candidate `emit` after preparation includes backend work, output-map
  construction and serialization. Worker JSON parsing and reply preparation
  account for part of the residual outside exported calls.

For finer attribution, apply `stages.patch` **only in an isolated scratch
worktree** at the candidate revision. It adds a WASM-only timing import and
scope markers; it deliberately does not belong to production compiler code.
It uses the already locked wasm-bindgen version. Build its WASM with the same
commands above, then run the same harness against that directory:

```sh
git apply tools/playground-bench/stages.patch
cargo build -p flick-wasm --target wasm32-unknown-unknown --release --locked
# Apply the same release overrides, Binaryen step and wasm-bindgen command
# as the ordinary build when comparing stage distributions.
node tools/playground-bench/index.mjs /tmp/flick-stages target/playground-perf/stages.json
```

`instrumentedStages` reports inclusive and exclusive times for parse,
resolve/check, lowering, backend, output-map construction and JSON string
serialization. Resolve/check includes lowering, so use its **exclusive** time
when adding stages. The backend can trigger builtin initialization on a cold
worker; discard warmups before interpreting steady-state attribution.
Instrumentation calls and timer resolution can dominate tiny stages; use
their counts and proportions to locate repeated work, and use uninstrumented
artifacts to decide whether a change improves observable latency.

The old worker source can also drive the instrumented candidate binary to
replay `update` followed by a cold `emit`. Label that as **legacy dispatch on
the instrumented candidate**, not an independently built baseline revision.
It helps attribute the removed frontend pass without conflating marker
overhead with the primary A/B result.

No timing hook changes language semantics, the reference model or Lean
contracts. The session candidate changes when existing analyses run and
which revision owns their memory, not type checking, lowering or execution.
