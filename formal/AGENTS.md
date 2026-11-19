# formal

The Lean 4 mechanization of the contracts in [docs/formal/](../docs/formal/)
(core calculus with regions, λRT, class elaboration, modules, memory).
[README.md](README.md) states what is proved, its assumptions and limits;
it is long, so read it by section.

## Layout

- `Formal.lean`, `FormalCert.lean` — library roots; `lake build` checks
  only the modules they import, directly or transitively.
- `Formal/Axioms.lean` — `#guard_msgs` pins of `#print axioms` for the main
  theorems.
- `Formal/RT/`, `Formal/Elab/` — λRT and the elaboration of classes.
- `FormalCert/` — rules and tactics for the certificates `flick-model`
  writes for its generated programs.

## Rules

- Follow the [formal gate](../docs/agents/validation.md#formal-gate): no
  `sorry` or new axioms, and the model and Lean change with the contract.
  A theorem here covers the calculus, not the Rust compiler or a runtime.
- Import a new file from a root, or from a module a root reaches.
- Pin the axioms of a new main theorem in `Formal/Axioms.lean` and list it
  under "What is proved" in the README.
- Files whose header names `just formal-schemes` or `just formal-certs`
  come from `flick-model`, and one that names `just formal-proofs` from the
  compiler's kernel; rerun that recipe instead of editing them.
- The Lake package has no dependencies (no Mathlib); the toolchain is
  pinned in `lean-toolchain`.

## Checks

- `just lean` (`lake build --wfail` in `formal/`).
- `just model` with it; after `just formal-schemes`, `just formal-certs` or
  `just formal-proofs` review the generated diff, then rebuild.
- `just model-certs` checks a batch of generated certificates with Lean.
