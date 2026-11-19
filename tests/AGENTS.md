# tests

Fixture suites read by the compiler's Rust tests, the e2e harness, the
reference model and both runtimes. Fixture formats and which suite covers
which behavior are in the
[testing guide](../docs/src/content/docs/contributing/testing.md).

## Layout

- `check/{pass,fail,projects}/` — checker fixtures.
- `run/` — programs with expected output; the `.txt` lists beside them
  select the targets and the model for each case.
- `rt/` — cases only Flick's own runtime runs (`../rt/` lines of
  `run/rt.txt`).
- `language/` — a Flick project pinning semantics JS and Go share.
- `syntax/` — grammar-only programs and `spec_examples.txt` pins.
- `ffi/` — packages for `flick ffi` and the `.d.ts` importer;
  `postgres/` — `just e2e-postgres`; `plugins/pairs/`,
  `backends/literal/` — the test plugin and backend crates.

## Rules

- Give a new `run/` case a `.stdout` derived from the contract, and list
  it in `run/portable.txt` when Go must run it. `just model` then requires
  it in `run/model.txt` (a subset of `portable.txt`) or in
  `run/model_gaps.txt` (`stdlib/` cases in `run/model_gaps_stdlib.txt`)
  with the reason it prints; `just conformance-rt`
  reports one that passes on Flick's runtime for `run/rt.txt`.
- `run/known_gaps.txt` cases must fail on their target; drop a line once
  the case passes there.
- Format `.fl` files with `flick fmt`, except under `check/fail/` and
  `check/projects/`, whose annotations point at columns.
- `check/projects/*/node_modules/` are tracked fixtures; keep them.

## Checks

- Checker: `cargo test -p flick-core --test check`.
- One run case, after `just e2e-prepare` and with `FLICK_STD_PATH` set:
  `FLICK_E2E_ONLY=<path part> cargo test --profile e2e -p flick --test e2e -- --ignored run_cases`.
- Lists and targets: `just e2e-cases`, `just model`, `just language`.
- Formatting: `cargo test -p flick-core --test corpus`.
