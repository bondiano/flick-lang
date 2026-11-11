# Subsystem ownership map

History contributors: bondiano, Vassiliy, chaoseternally — default-branch non-bot author names, not appointed roles.

This map helps route a question to contributors with relevant repository history. No `CODEOWNERS` file was found in the inspected checkout, so this is not an enforced reviewer list, a ranking, or a claim that anyone currently accepts maintenance responsibility.

## Reading the evidence

The source survey used path-scoped history on **`origin/main`**, not the dirty current branch: `git log origin/main --format=%an -- <paths>` for author names, alongside commit dates. Bots/automation identities were excluded, and the table retains the top two or three recent/frequent non-bot names for each broad subsystem without publishing per-person counts.

Display names use usernames or first names: **Vassiliy** corresponds to the recorded author name **Vassiliy Kuzenkov**. `bondiano` and that author name may identify the same person; this page does not merge identities or infer a bus factor. A historical name is evidence of work in a path, not a guarantee of availability or exclusive ownership.

## Routing map

| Subsystem | Relevant default-branch names | Source scope | Reading path |
| - | - | - | - |
| Compiler and shared IR | bondiano, Vassiliy | `flick-core/src/syntax/`, `flick-core/src/resolve/`, `flick-core/src/types/`, `flick-core/src/ir/`, `flick-core/src/opt/`, `flick-core/src/codegen/`, `flick-ir/` | [Compiler](systems/compiler/index.md) |
| CLI, project model, editor protocols | bondiano, Vassiliy | `flick/src/`, `flick-core/src/project/`, `flick-core/src/ide/`, `flick-core/src/ffi/` | [Project and CLI](systems/project-and-cli/index.md) |
| JavaScript/Effect/Go runtimes | bondiano, Vassiliy | `runtime/`, `packages/effect-ts/`, `runtime-go/` | [Runtimes](packages/runtimes/index.md) |
| Libraries and plugins | bondiano, Vassiliy | `packages/core/`, `packages/std/`, `flick-plugin/`, `plugins/` | [Libraries](packages/libraries.md), [plugins](packages/plugins.md) |
| Browser compiler, site, editors | bondiano, Vassiliy | `flick-wasm/`, `docs/src/`, `editors/` | [Browser tools](systems/browser-tools.md) |
| Reference model and proofs | bondiano, Vassiliy | `flick-model/`, `formal/`, `docs/formal/` | [Formal model](systems/formal-model.md) |
| Developer tooling and CI | bondiano, Vassiliy, chaoseternally | `tools/`, `.github/workflows/`, `justfile`, `nix/`, `flake.nix` | [Developer tools](systems/developer-tools.md) |

The survey recorded last default-branch activity on **2026-11-10** in each of the first six broad groups. That is a subsystem date, not a per-person last-active date. The tooling/CI row adds the separately observed `chaoseternally`; no separate last-activity date is asserted for that name.

## Cross-subsystem changes

| Change | Coordinate across |
| - | - |
| Syntax or diagnostics | Parser/checker, formatter, IDE, editor grammars, fixtures, English/Russian docs |
| Effects, regions, ownership | Checker, lowering, JS/Effect/Go runtimes, model/formal contract |
| Interface or IR schema | Compiler, package caches/publication, backends, plugin host, reference-model translation |
| Plugin SDK or WIT | SDK/macros, core/native/WASM host conversions, first-party plugins, permissions/cache tests |
| Runtime dependency or ABI | Runtime packages, codegen routing/bridges, CLI compatibility constants, conformance tests |
| Browser API or capability split | Core features, `flick-wasm/`, worker interoperability, bundle budgets |

Agree on file ownership and contract changes before sharing implementation work; separate worktrees do not remove design conflicts. Repository policy is in `AGENTS.md`, `CONTRIBUTING.md`, and `docs/agents/compiler-workflow.md`.

## Limits and follow-up

The working tree contains substantial pre-existing uncommitted source changes. The history map intentionally does not attribute those changes, infer their author from the logged-in user, or treat them as default-branch activity. It includes no private research notes or email addresses.

For a current reviewer/maintainer commitment, confirm with the project rather than promoting this historical routing table into policy. Start implementation work with the [contribution conventions](how-to-contribute/patterns-and-conventions.md) and use the [shared model reference](reference/data-models.md) to identify contract owners.
