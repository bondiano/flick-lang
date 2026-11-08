# Working on Flick

These instructions apply throughout the repository. Follow more specific
`AGENTS.md` files where present.

## Start here

- Read [CONTRIBUTING.md](CONTRIBUTING.md) for setup and conventions.
- Use the [validation matrix](docs/agents/validation.md) to select checks.
- For compiler work, follow the [compiler workflow](docs/agents/compiler-workflow.md).
- Inspect the current branch and working tree before editing. Treat existing
  changes and other worktrees as someone else's work; do not overwrite or clean them.
- Local research notes are context, not proof of what is implemented. Verify
  relevant claims against this checkout. Do not publish private plans or notes.

## Repository map

| Area | Location |
| - | - |
| Parser, resolver, checker | `flick-core/src/{syntax,resolve,types}/` |
| Lowering and shared IR | `flick-core/src/ir/`, `flick-ir/` |
| Optimization and backends | `flick-core/src/{opt,codegen}/` |
| CLI, project execution, LSP | `flick/src/`, `flick-core/src/{project,ide}/` |
| Browser compiler | `flick-wasm/` |
| Standard library and plugins | `packages/`, `plugins/`, `flick-plugin/` |
| JS and Go runtimes | `runtime/`, `runtime-go/` |
| Reference model and proofs | `flick-model/`, `formal/`, `docs/formal/` |
| Fixtures and integration tests | `tests/`, `flick-core/tests/`, `flick/tests/` |
| Site and playground | `docs/src/`, `tools/astro-flick/` |
| Editor grammars | `editors/` |

## Preserve the contracts

- Reproduce a bug and add a regression test before fixing it. Assert the
  expected behavior, not just agreement between two implementations.
- Syntax changes must account for parsing, formatting, editor grammars,
  diagnostics/fixes, fixtures and English/Russian documentation.
- Changes to typing, effects, regions or execution semantics must account for
  the reference model and Lean. Update them when their contract changes;
  otherwise explain why they are unaffected. A model proof is not a proof of
  the Rust compiler or either runtime.
- Preserve diagnostic codes, spans and safe fixes unless their change is
  intentional. Review snapshot differences; do not bulk-accept them blindly.
- Do not weaken tests, add known gaps, loosen budgets or remove proof
  obligations merely to pass checks. Propose a contract change explicitly.
- Keep interactive documentation behavior in Flick; use TypeScript for
  browser/compiler interoperability and mounting.
- Do not commit, push, publish or merge unless requested.

## Finish and hand off

Run the checks for every affected area, not just the edited files. Report
the change, affected guarantees, exact checks and outcomes, skipped checks
with reasons, and remaining risks. Do not call unrun checks passing.
When work is explicitly shared between agents, agree on file ownership and
contract changes first; a separate worktree does not prevent design conflicts.
