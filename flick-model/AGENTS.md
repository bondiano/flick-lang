# flick-model

The reference checker and small-step interpreter of λFlick, specified in
[docs/formal/](../docs/formal/). It translates flick-core's checked IR so
it can run the corpus, generates programs for properties, checks runtime
traces and writes the generated Lean files of `formal/`.

## Layout

- `src/check/`, `src/eval/` — statics and dynamics, one function per rule
  of the formal docs, named after it.
- `src/translate/` — checked IR to λFlick; a node with no λFlick form
  becomes `Term::Stuck`. `translate/lean.rs` writes the Prelude schemes.
- `src/generate/`, `src/cert/` — well-typed program generator and the Lean
  certificates of its programs (`formal/FormalCert/`).
- `src/memory/` — counted heap for the ownership IR; `src/rt/` — λRT trace
  monitor (`examples/rt_trace.rs`); `src/modules.rs` — the executable
  counterpart of `formal/Formal/HigherModules.lean`.

## Rules

- Implement the rules of `docs/formal/`, not flick-core's behavior; change
  a rule's page, its code here and the Lean proof together.
- Each `tests/run` case is listed in `tests/run/model.txt` (and in
  `portable.txt`) or in `tests/run/model_gaps.txt` with its reason. Move a
  case to `model.txt` once it runs.
- `tests/known_gaps.txt` and `tests/cert_gaps.txt` only shrink.
- Regenerate Lean output with `just formal-schemes` or `just formal-certs`;
  golden tests here fail when it is stale.

## Checks

- `just model`. A plain `cargo test -p flick-model` skips the corpus,
  fixture and property tests, which are `#[ignore]`; `PROPTEST_CASES` sets
  the number of property cases.
- Focused: `cargo test -p flick-model --test model <filter>`, `--test run`,
  `--test certs`, or `--lib <module>::`.
- `just lean` after regenerating Lean files; `just model-certs` to check a
  batch of generated certificates with Lean.
- See the [formal gate](../docs/agents/validation.md#formal-gate).
