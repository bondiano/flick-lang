# Flick

Flick is an alpha language for browser, server, and command-line applications. This wiki explains the compiler, generated programs, runtime libraries, and contributor workflow in this repository.

The native entry point is `flick/src/main.rs`. It uses the shared compiler in `flick-core/src/lib.rs`; `flick-wasm/src/lib.rs` exposes that core to the browser. The language has explicit function signatures, typed errors, services, effects, classes, and region checking, implemented under `flick-core/src/types/`.

## Reading paths

- [Architecture](architecture.md) follows source through checking, lowering, optimization, and emission.
- [Getting started](getting-started.md) gives the supported development setup.
- [Compiler](../systems/compiler/index.md) explains the compiler's internal boundaries.
- [Features](../features/index.md) connects language capabilities to their implementations.
- [Contributing](../how-to-contribute/index.md) explains validation and change requirements.
- [Statistics](../by-the-numbers.md) and [history](../lore.md) describe this checkout and its evolution.

## Snapshot scope

Generated on 2026-11-10 from the current working tree on `main`. The tree contains uncommitted compiler, plugin, runtime, and tooling changes. Git revision metadata identifies the base commit, not a clean or tested release. Source links to that commit may not include uncommitted files.

The language specification lives in `docs/src/content/docs/spec/`; `docs/formal/` and `formal/README.md` describe the narrower formal contract. A proof about the model is not a proof about generated JavaScript or Go.
