# Validation for agent-assisted changes

Use this matrix with the [compiler workflow](compiler-workflow.md).
Setup is in [CONTRIBUTING.md](../../CONTRIBUTING.md); fixture formats are in
the [testing guide](../src/content/docs/contributing/testing.md).

## Choose by behavior, not just paths

1. List the contracts the change can affect. Apply every matching row below.
2. Run the smallest regression test while developing, then the area checks.
3. Before handing off executable-code changes, run `just check` and the
   relevant integration checks. Broad compiler/runtime transitions also need
   `just e2e`, `just model` and the Lean build when its contract is affected.
4. If a tool, platform or service is unavailable, report the exact check and
   blocker. This is incomplete validation, not a pass. Do not install tools,
   start services or use external credentials without the required permission.

Commands run from the repository root unless stated otherwise. `just` sets
`FLICK_STD_PATH` to this checkout's std. For direct compiler or Cargo tests
that load std, use `export FLICK_STD_PATH="$PWD/packages/std"` in the same
shell. Use pinned toolchains and existing scripts, not improvised substitutes.
`just check` covers lint and Rust tests/capability checks; it does not include
the separate model, Lean or e2e gates.

| Changed behavior | Focused checks and required coverage |
| - | - |
| Parser or formatter | `cargo test -p flick-core --test corpus` and the relevant syntax/fmt unit tests. Update `tests/syntax/`, parser recovery cases, both editor grammars and applicable spec-example pins. Run `just tree-sitter` when its grammar changes. |
| Types, effects, regions, classes | `cargo test -p flick-core --test check`, relevant checker unit tests, `just model`, `just language`. Cover accepted and rejected programs, diagnostic codes/spans and fixes. Apply the formal gate below when semantics change. |
| IR, lowering or optimization | `cargo test -p flick-ir`, `cargo test -p flick-core --test ir_round_trip`, relevant lowering/opt unit tests, `just e2e-tests`, `just e2e-cases`, `just language`. Check debug/release equivalence, JS/TS output and portable Go behavior. |
| Runtime, cancellation or cleanup | `pnpm -C runtime typecheck`, `pnpm -C runtime lint`, `pnpm -C runtime test`, `just go-test`, `just conformance`, `just language`; `just go-oracles check` when a file of `runtime-go/flickrt` that `tools/go-*` generates, the Effect pin or Node changes. Cover success, failure, interruption, overlapping work, finalizer order and exactly-once cleanup. Apply the formal gate if the contract changes. |
| Plugins or their host | `cargo test -p flick --test plugins`, relevant SDK/core/plugin unit tests, `just wasm-plugins`, `just e2e-tests`. Cover native and WASM execution, diagnostics, permissions, offline behavior and cache invalidation as applicable. |
| Packages, builds or CLI | `just e2e-tests` and relevant project/cache/CLI unit tests. Cover existing projects, locked/offline behavior, changed dependency inputs and emitted artifacts. |
| LSP or shared IDE queries | Relevant `flick-core` IDE and `flick` LSP tests, `just e2e-js` if browser queries are affected. Exercise open/edit/query/close, stale results and UTF-16 positions. |
| Documentation content | `cargo test -p flick-core --test corpus docs_examples_check`, `pnpm -C docs test`; build with `just docs`, then `node docs/scripts/links.mjs` for site navigation/content changes. Keep hand-written site pages paired in en/ru. |
| Playground, widgets or browser compiler | `just e2e-js`, `just bundle-size` (`just e2e-js` includes the WASM size gate). Check startup, editing, diagnostics, running, cancellation and applicable preview/share behavior. Preserve worker isolation, preview sandboxing and refusal to auto-run shared code. |
| Cargo capabilities | `just feature-check`, `just feature-test`, `just wasm`. Workspace feature unification alone does not validate minimal consumers. |
| Performance or bundle size | Run the applicable benchmark before and after on equivalent inputs, plus `just bundle-size` and `just ratchet` for affected size budgets. Use `just perf` for its tracked compiler/LSP/server metrics on a supported host. Report actual measurements, not budget values as measurements. |

Run UI/browser checks for new states, not only the happy path. SQL changes
that reach PostgreSQL need `just e2e-postgres`; full-stack changes affecting
Conduit need `just conduit-api` and `just conduit-e2e`. These require the
dependencies described in the justfile (including Docker and browser tools).

## Formal gate

Read [what is proved and its limits](../../formal/README.md) and the affected
[formal contract](../formal/). For a change to that contract:

- Update the reference model, translation, examples and Lean statements/proofs
  together with the implementation. Extend tests for the new behavior.
- Run `just model` and `just lean` (`lake build --wfail` in `formal/`).
- If Prelude schemes change, run `just formal-schemes`, review the generated
  diff, then rerun `just model` and the Lean build.
- Do not introduce `sorry`, extra axioms or narrower theorem premises to
  conceal a broken guarantee. State unsupported cases explicitly for review.

A compiler bug fix that restores an unchanged formal contract may need no
proof edits. Explain that distinction and test the implementation regression.

## Keep expectations independent

- Checker failures belong in annotated `tests/check/fail/` fixtures when codes
  and spans matter. Pass fixtures must remain diagnostic-free.
- Execution cases in `tests/run/` need independently justified `.stdout`,
  errors and exit status. Use `tests/language/` for shared JS/Go semantics.
- `flick fail CODE` Markdown blocks pin diagnostic codes, including plugin
  codes such as `std/Q001`. Use annotated fixtures to guard spans and fixes
  as well as a specific rejection.
- Check model-supported cases against the model as well as both runtimes.
  Backend agreement alone can preserve the same bug twice.
- Inspect snapshot and generated-file changes before accepting them. Never
  replace expected results with observed output without a contract-based reason.

## Documentation-only harness changes

For `AGENTS.md` and `docs/agents/` prose with no executable changes, check
relative links, referenced paths and commands against the checkout, then run
`git diff --check`. These files are not localized site pages. Compiler,
browser and Lean suites are not required merely for editing this guidance.
