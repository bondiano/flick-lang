# Proof editor workload

Run `just lsp-proof-bench` from a clean checkout to build a release compiler and measure it over stdio. Record the checkout revision with the output. The report records the binary SHA-256, CPU, operating system and Node version.

```sh
git rev-parse HEAD
just lsp-proof-bench 30 100
```

The fixture links three proof modules through an imported implementation. It also loads the canonical Option law bundle and checks that the inherited Functor law is visible. The benchmark alternates a valid and a failing proof body, then edits just the implementation of a dependency without changing its signature. It waits for the proof status on the current revision and checks that a superseded edit or closed buffer cannot expose an old checked proof.

The report gives spawn-to-initialize time and first proof-open time separately. Warm request, explicit proof search and edit timings include p50 and p95. The default is five warm-up samples. The workload also applies a checked search result, verifies that a later revision invalidates it, and times hover after cancelling a search. The soak reports resident memory after repeated open, edit and close cycles in one workspace and in fresh workspaces. Linux uses the process high-water mark for peak RSS. macOS samples RSS during the session.

For a prebuilt compiler, use the same stdlib as its checkout.

```sh
FLICK_STD_PATH="$PWD/packages/std" node tools/lsp-bench/proofs.mjs --samples 30 --cycles 100 target/release/flick
```

Use `--cycles 0` to skip the soak and `--status-only` to measure just goals and verification.

## Baseline

Measured on 2026-11-24 with an Apple M4 Max, macOS 26.6.2 (25G83, arm64),
Rust 1.95.0 and Node 24.18.0. The measured checkout was
`635a41cc85ac31468f444551762167f8bbd564fd` with source tree
`16b36473cc23a3eec05290e43b926c517408ea89`. The release binary's SHA-256 was
`6d820d0907d4723a5df30d4285cf8d4417894b008dbedcddbb4884ad953ff8d9`.

```sh
export XDG_CACHE_HOME="$(mktemp -d)"
just lsp-proof-bench 30 100
just lsp-bench
just lsp-soak
```

The proof run started with an empty disk cache and used the checked-in
`fixtures/proofs` and `packages/core/proofs/Laws/Option.fl`. Each latency
row uses 30 samples after five warm-up requests. Edit latency includes the
LSP debounce and the wait for the current generation to finish.

| Request | p50 (ms) | p95 (ms) |
| --- | ---: | ---: |
| Warm proof status | 0.075 | 0.124 |
| Warm proof hover | 0.063 | 0.082 |
| Warm goal completion | 0.103 | 0.140 |
| Explicit proof search | 23.54 | 25.26 |
| Fresh status after a proof-body edit | 35.99 | 38.12 |
| Fresh status after a same-signature dependency-body edit | 35.87 | 38.38 |

The first process took 1,988 ms to initialize and 9,701 ms to report its
first checked proof after opening the file. The case-split proof `rebuild`
used 16 normalization steps. The `option` bundle used 1,114 across its law
fields. Hover answered in 0.112 ms after cancellation and a superseding edit
cancelled a search in 0.076 ms. These are individual observations.

Peak sampled proof RSS was 277.0 MiB. After 100 cycles in the same workspace,
RSS was 276.67 MiB, equal to its value at the start of that phase. After 100
fresh workspaces it settled at 273.45 MiB by cycle 50. The general `lsp-bench`
with the warmed disk cache reported a 3,609 ms first open, a 59 ms repeated
open and a 71 ms hover after edit, with 182.1 MiB peak RSS. In the general
soak, 400 distinct files ended at 37.0 MiB with 0.0 MiB growth in that phase.

For the CPU profile, rebuild the same release with symbols using
`cargo rustc --locked --release -p flick --bin flick -- -C strip=none`. Run the
proof workload with `--samples 400 --warmup 5 --cycles 0 --status-only` and
sample its LSP child with macOS `sample` for 12 seconds at 1 ms intervals while
it processes repeated edits. The symbolized binary's SHA-256 was
`33c333c39680ebc8efd393db387a3f7a437f5323c6ecedaf516fb8bd103bb60b`.
Of 6,023 project analysis and load stack samples, 67 included lemma replay
and 80 included kernel normalization, with 20 in both. Together they accounted
for 2.1% of these samples. This workload supports keeping independent replay
on each revision and checking the current implementation bodies on every edit.
