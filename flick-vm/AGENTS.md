# flick-vm

The virtual machine for Flick's ownership IR (`flick-ir/src/memory/`),
starting with `tier0`: the reference counted heap and executor specified in
[docs/formal/memory.md](../docs/formal/memory.md).

## Layout

- `src/lib.rs` — crate entry exporting `pub mod tier0`.
- `src/tier0.rs`, `src/tier0/` — reference counted heap, pattern matcher,
  primitives, program/fiber/scope/layer executor, and `host_bridge/`
  (Node worker scripts and wire codec for bridged foreign calls).

## Rules

- Keep every file under `flick-vm/` at or below 20,000 bytes; split at
  coherent module boundaries rather than growing monolithic files.
- `tier0` executes explicit `dup`/`drop`/`reuse` instructions as written.
  `Heap::finish` checks for zero live slots and never sweeps leaks.
- Changes to ownership, scope-close, arena, or reuse semantics must keep
  [docs/formal/memory.md](../docs/formal/memory.md) and the Lean proofs in
  `formal/Formal/Memory*.lean` aligned.

## Checks

- `cargo test -p flick-vm -- --include-ignored`
- `just model` (runs both `flick-vm` and `flick-model`, including corpus and
  counted-memory integration suites).
- `just check` and `just lean` when formal memory contracts are touched.
