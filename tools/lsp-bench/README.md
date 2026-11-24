# Proof editor workload

Run `just lsp-proof-bench` from a clean checkout to build a release compiler and measure it over stdio. Record the checkout revision with the output. The report records the binary SHA-256, CPU, operating system and Node version.

```sh
git rev-parse HEAD
just lsp-proof-bench 30 100
```

The fixture links three proof modules through an imported implementation. It also loads the canonical Option law bundle and checks that the inherited Functor law is visible. The benchmark alternates a valid and a failing proof body, then edits just the implementation of a dependency without changing its signature. It waits for the proof status on the current revision and checks that a superseded edit or closed buffer cannot expose an old checked proof.

The report gives spawn-to-initialize time and first proof-open time separately. Warm request and edit timings include p50 and p95 after five warm-up samples. The soak reports resident memory after repeated open, edit and close cycles in one workspace and in fresh workspaces. Linux uses the process high-water mark for peak RSS. macOS samples RSS during the session.

For a prebuilt compiler, use the same stdlib as its checkout.

```sh
FLICK_STD_PATH="$PWD/packages/std" node tools/lsp-bench/proofs.mjs --samples 30 --cycles 100 target/release/flick
```

Use `--cycles 0` to skip the soak and `--status-only` to exercise the goal and verification requests without hover or completion.
