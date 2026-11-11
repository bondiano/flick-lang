# Contributing to Flick

Active contributors: bondiano, Vassiliy Kuzenkov — nonbot author names recorded on `origin/main` for contributor guidance and the task runner.

Start with a small, independently justified regression, follow the behavior through its real entry point, and validate every affected contract. Repository rules live in `AGENTS.md`, `CONTRIBUTING.md`, `docs/agents/compiler-workflow.md`, and `docs/agents/validation.md`.

## Choose a guide

| Goal | Guide |
| - | - |
| Set up a change and prepare its handoff | [Development workflow](development-workflow.md) |
| Find a harness or write a diagnostic/runtime fixture | [Testing](testing.md) |
| Diagnose a compiler, project, plugin, editor, or runtime failure | [Debugging runbook](debugging.md) |
| Choose tools, recipes, generators, and performance checks | [Tooling](tooling.md) |
| Understand coding and documentation rules | [Patterns and conventions](patterns-and-conventions.md) |
| Understand CI, release tags, installers, and Nix | [Deployment and distribution](../deployment.md) |
| Understand what the toolchain trusts | [Security boundaries](../security.md) |

For the implementation map, use [architecture](../overview/architecture.md) and the [systems index](../systems/index.md). User-facing language documentation remains under `docs/src/content/docs/`; this wiki explains the checkout rather than replacing that reference.

## Bootstrap

Run commands below from the repository root. The pins are Rust **1.95.0** in `rust-toolchain.toml`, Node.js **24.18.0 or newer**, and pnpm **12.16.1** in `package.json`. `CONTRIBUTING.md` documents installation of `just`, `cargo-machete`, and `cargo-nextest`; `flake.nix` offers `nix develop` as an alternative tool environment.

```sh
pnpm install --frozen-lockfile
just lint
just test
```

These commands install dependencies and run checks; they are instructions, not work performed while generating this wiki. Go 1.23 or newer is needed for the Go target; Lean uses the separate `formal/lean-toolchain` pin. Browser, Docker/PostgreSQL, and full-stack suites have additional prerequisites described in [testing](testing.md).

`justfile` exports `FLICK_STD_PATH` to this checkout's `packages/std`. Direct Cargo/compiler invocations that need the local standard library should set it in the same shell:

```sh
export FLICK_STD_PATH="$PWD/packages/std"
cargo test -p flick-core --test check
```

This avoids accidentally testing against a released implicit package rather than the source being changed.

## Nonnegotiable contracts

- Inspect the branch, working tree, and local instructions before editing. Leave other tasks' changes intact, and agree on file ownership when working concurrently.
- Reproduce a bug and add a failing regression before the fix. Expected values, diagnostics, spans, and safe fixes must come from the contract, not from copying current output.
- Syntax reaches formatting, diagnostic fixes, editor grammars, fixtures, and English/Russian site documentation.
- Typing, effects, regions, and execution changes must account for `flick-model/`, `docs/formal/`, and `formal/`. Explain when a fix restores an unchanged contract and needs no proof edits.
- Never weaken tests, budgets, or proof obligations to hide a regression. Do not commit, push, publish, or merge without a request.

`just check` means **lint plus test**, not the whole CI graph. Its unit suite excludes `flick-model`; model, Lean, and end-to-end checks are separate gates. See [the testing recipe map](testing.md#recipe-boundaries).

## Snapshot and evidence

These pages describe the inspected dirty working tree on `main`, based at `6e9096fb`, not a tested release. Source changes were already in progress during wiki generation; re-read implementations and recipes before relying on an uncommitted detail. In particular, plugin hosting and runtime/export ownership include working-tree changes.

Contributor bylines summarize author-name strings in scoped `git log origin/main --format=%an -- <paths>`, excluding bots. Names are not an identity merge or an ownership assignment.

The contributor pages were validated as Markdown/source navigation only. No compiler, runtime, browser, Nix, or Lean suite was run for this wiki-only work.

## Key source files

| Source | Why read it |
| - | - |
| `AGENTS.md` | Repository-wide contracts, ownership, commits, and handoff rules |
| `CONTRIBUTING.md` | Tool setup, capability flags, site development, release overview |
| `docs/agents/compiler-workflow.md` | Reproduce → regression → implementation → validation |
| `docs/agents/validation.md` | Behavior-based check selection and formal gate |
| `justfile` | Actual recipe dependencies and environment |
| `docs/src/content/docs/contributing/testing.md` | Fixture formats and suite explanations |

Known prose/implementation discrepancies are recorded under [documentation cleanup opportunities](../cleanup-opportunities.md), rather than silently copying older descriptions.
