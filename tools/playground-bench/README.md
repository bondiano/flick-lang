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

## Recorded decision for #876

The [measured report](results/876/README.md) retains the production session
and worker implementation. The direct-emission candidate did not show a
repeatable browser latency benefit and increased diagnostics-only linear
memory capacity. The raw Chromium and separate Node attribution reports are
committed beside that decision.

`direct-emit.patch` preserves the experimental production delta. Apply it
only in an isolated worktree for further experiments. It adds the candidate
`prepare_emit` API and its worker dispatch; neither is active in production.
The report pins the historical baseline and candidate commits used for the
recorded A/B test. Applying this patch to a later main checkout starts a new
experiment and does not recreate those historical compiler inputs.

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
source. Install the pinned binding CLI in each isolated worktree, as the
repository's `docs-wasm` recipe does, and build with identical settings:

```sh
cargo install wasm-bindgen-cli --version 0.2.130 --locked --root target/tools
CARGO_PROFILE_RELEASE_OPT_LEVEL=z CARGO_PROFILE_RELEASE_LTO=fat \
  CARGO_PROFILE_RELEASE_STRIP=debuginfo \
  cargo build -p flick-wasm --target wasm32-unknown-unknown --release --locked
node tools/wasm-dedup/index.mjs target/wasm32-unknown-unknown/release/flick_wasm.wasm
target/tools/bin/wasm-bindgen --target web --out-name flick_wasm --out-dir /tmp/flick-before \
  target/wasm32-unknown-unknown/release/flick_wasm.wasm
cp docs/src/playground/compiler.worker.ts /tmp/flick-before/compiler.worker.ts
```

Repeat those commands in the candidate worktree, changing the output paths
to `/tmp/flick-after`. Preserve each worker source along with its own WASM.

Run the same harness against both saved artifacts, with no competing builds
or benchmark processes. Repeat in reverse order to expose drift:

```sh
BUILD_REVISION=<baseline-sha> \
  WORKER_SOURCE=/tmp/flick-before/compiler.worker.ts \
  node tools/playground-bench/index.mjs /tmp/flick-before target/playground-perf/before.json
BUILD_REVISION=<candidate-sha> \
  WORKER_SOURCE=/tmp/flick-after/compiler.worker.ts \
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
The Rust session regressions compare a persistent `update` + `emit` session
against fresh cold runs and check lazy editing, profile reuse and the lifetime
of the previous emitted revision.

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
worktree**. To inspect the candidate, first check out its historical revision
or apply `direct-emit.patch` to a separate checkout of this revision. The
stage patch adds a WASM-only timing import and
scope markers; it deliberately does not belong to production compiler code.
It uses the already locked wasm-bindgen version. Build its WASM with the same
commands above, then run the same harness against that directory:

```sh
git apply tools/playground-bench/stages.patch
cargo install wasm-bindgen-cli --version 0.2.130 --locked --root target/tools
CARGO_PROFILE_RELEASE_OPT_LEVEL=z CARGO_PROFILE_RELEASE_LTO=fat \
  CARGO_PROFILE_RELEASE_STRIP=debuginfo \
  cargo build -p flick-wasm --target wasm32-unknown-unknown --release --locked
node tools/wasm-dedup/index.mjs target/wasm32-unknown-unknown/release/flick_wasm.wasm
target/tools/bin/wasm-bindgen --target web --out-name flick_wasm --out-dir /tmp/flick-stages \
  target/wasm32-unknown-unknown/release/flick_wasm.wasm
WORKER_SOURCE="$PWD/docs/src/playground/compiler.worker.ts" \
  node tools/playground-bench/index.mjs /tmp/flick-stages target/playground-perf/stages.json
```

`instrumentedStages` reports inclusive and exclusive times for parse,
resolve/check, the `lower_if_clean` scope, backend, output-map construction and
JSON string serialization. The lowering marker includes its eligibility
guard; an entry does not by itself prove that IR was materialized.
Resolve/check includes that scope, so use its **exclusive** time when adding
stages. The backend can trigger builtin initialization on a cold
worker; discard warmups before interpreting steady-state attribution.
Instrumentation calls and timer resolution can dominate tiny stages; use
their counts and proportions to locate repeated work, and use uninstrumented
artifacts to decide whether a change improves observable latency.

The old worker source can also drive the instrumented candidate binary to
replay `update` followed by a cold `emit`. Label that as **legacy dispatch on
the instrumented candidate**, not an independently built baseline revision.
It helps attribute the removed frontend pass without conflating marker
overhead with the primary A/B result.

The experiment concerns when existing analyses run and which revision owns
their memory. The final production implementation, language semantics,
reference model and Lean contracts are retained.
