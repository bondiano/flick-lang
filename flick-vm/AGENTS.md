# flick-vm

The virtual machine for Flick's ownership IR (`flick-ir/src/memory/`), split
into a tier-neutral standard primitive/runtime/host library (`stdlib`) and the
reference-counted tier-0 executor (`tier0`) specified in
[docs/formal/memory.md](../docs/formal/memory.md).

## Layout

- `src/lib.rs` — crate entry exporting `pub mod stdlib`, `pub mod tier0`, and
  `pub mod tier1`.
- `src/stdlib.rs`, `src/stdlib/` — tier-neutral shared primitive library,
  caller-supplied fuel/allocation/host hooks (`hooks.rs`: `Accounting`,
  `HostEnv`, `VmHooks`, `FuelCounter`, `CountingHooks`, `HostContext`,
  `Unmetered`), pure primitives (`primitives/`), shared CLI command/parser/help/
  completion/runner engine (`cli.rs`, `cli/`), Stage 4 cooperative runtime
  helpers (`runtime/{context,coop,layers}.rs`), Stage 5 host modes (`host.rs`),
  and the persistent Node `HostBridge` (`host_bridge.rs`, `host_bridge/`).
- `src/tier0.rs`, `src/tier0/` — reference counted heap, pattern matcher, and
  program/fiber/scope/layer executor. Re-exports `HostBridge`, `HostMode`, and
  deterministic host constants from `stdlib` for API compatibility.
- `src/tier1/` — register bytecode compiler (`prepare.rs`, `prepare_ops.rs`,
  `prepare_ops_data.rs`, `prepare_fuse.rs`), opcode/error types (`opcode.rs`),
  register machine (`machine.rs`, `machine_ops.rs`, `machine_cli.rs`,
  `machine_dict.rs`, `machine_call.rs`, `machine_recovery.rs`, etc.), and
  bytecode test runner (`test_runner.rs`) running over `tier0::Heap` and
  `crate::stdlib`.
- `tests/kernel_primitives.rs` — direct primitive-kernel boundary tests against
  `flick_vm::stdlib` and its accounting/host hooks.
- `tests/tier1_numeric.rs` — numeric-policy boundary tests, unsupported-operation
  `PrepareError` span tests, intra-block fusion, tail-call frame reuse, and
  zero-leak checks for `tier1`.

## Rules

- Keep every file under `flick-vm/` at or below 20,000 bytes; split at
  coherent module boundaries rather than growing monolithic files.
- All std primitives, Stage 4 pure context/layer helpers, and Stage 5 host
  modes/bridge belong under `src/stdlib/`, never inside `src/tier0/` or
  `src/tier1/` executor modules. Both `tier0` and `tier1` call only
  `crate::stdlib` through its `Accounting`/`VmHooks` adapters.
- `tier0` executes explicit `dup`/`drop`/`reuse` instructions as written.
  `Heap::finish` checks for zero live slots and never sweeps leaks.
- `tier1` must never silently fall back to `tier0`; any reachable unsupported
  operation must fail `tier1::prepare` with `PrepareError::Unsupported` naming
  the operation and its source span. Keep one `translate_*` function per IR
  operation so `#980` Lean proofs map directly onto each translator.
- Changes to ownership, scope-close, arena, or reuse semantics must keep
  [docs/formal/memory.md](../docs/formal/memory.md) and the Lean proofs in
  `formal/Formal/Memory*.lean` aligned.

## Checks

- `cargo test -p flick-vm -- --include-ignored`
- `just model` (runs both `flick-vm` and `flick-model`, including corpus,
  kernel conformance, and counted-memory integration suites).
- `just kernel` (verifies JS, Go, and `flick-vm`'s shared primitive library
  against `tests/kernel` and `tests/kernel/language.txt`).
- `just check` and `just lean` when formal memory contracts are touched.
