# How this checkout took shape

The reachable history begins on **2026-10-22**, less than three weeks before this wiki's 2026-11-10 snapshot. It records a compiler growing into a multi-target application toolchain, followed by extraction of language libraries, plugins, shared IR, and a Flick-owned runtime. This page describes evidence in Git, not a reconstruction of private plans or an assertion that old commit messages prove current behavior.

Dates below use **committer time normalized to UTC**. Some commits displayed locally as November 10 have UTC dates on November 9. Revision links refer to the repository history; current source paths refer to the dirty working tree. See [the numbers](by-the-numbers.md) for measurement rules and [architecture](overview/architecture.md) for the current system rather than its chronology.

## Eras visible in the history

| UTC dates | What the history establishes | Evidence |
| - | - | - |
| **2026-10-22: compiler skeleton to application primitives** | The initial Cargo CLI/core workspace is followed by the browser crate, lossless syntax/formatter, module resolution, type checking, effects, classes, regions, typed IR, ESM/TypeScript generation, runtime adapter, and standard library/test runner. | [`4e77f06a`](https://github.com/bondiano/flick-lang/commit/4e77f06a), [`21f72977`](https://github.com/bondiano/flick-lang/commit/21f72977), [`e7327dfa`](https://github.com/bondiano/flick-lang/commit/e7327dfa), [`11e5199d`](https://github.com/bondiano/flick-lang/commit/11e5199d), [`9acb7da9`](https://github.com/bondiano/flick-lang/commit/9acb7da9), [`e3e9d6e2`](https://github.com/bondiano/flick-lang/commit/e3e9d6e2) |
| **2026-10-23–28: application tooling and UI evolution** | SQL and UI DSLs, editor integrations/LSP, the Astro documentation/playground, and an expanded Effect-facing library appear. UI roots move from the Ref-driven API to atoms; server template rendering gains a path without React. | [`454bcf6e`](https://github.com/bondiano/flick-lang/commit/454bcf6e), [`7d43088a`](https://github.com/bondiano/flick-lang/commit/7d43088a), [`f4d3bc33`](https://github.com/bondiano/flick-lang/commit/f4d3bc33), [`5daf1438`](https://github.com/bondiano/flick-lang/commit/5daf1438), [`57a7b63b`](https://github.com/bondiano/flick-lang/commit/57a7b63b) |
| **2026-10-29–2026-11-02: another target and independent semantic models** | A Go executable backend arrives, followed by runtime coverage work. The Rust reference checker/interpreter arrives October 31; Lean core mechanization and runtime/model work follow November 1. October 30 also introduces the shared arena pretty printer. | [`63a45c74`](https://github.com/bondiano/flick-lang/commit/63a45c74), [`edf97f35`](https://github.com/bondiano/flick-lang/commit/edf97f35), [`cf94115a`](https://github.com/bondiano/flick-lang/commit/cf94115a), [`33b97174`](https://github.com/bondiano/flick-lang/commit/33b97174) |
| **2026-11-03–06: move contracts out of compiler internals** | Builtin language modules move to `packages/core/` and `packages/std/`. The plugin interface arrives November 4; CLI, server declarations, and SQL tables move into first-party plugins. Typed IR data is extracted into `flick-ir/` on November 6. | [`35d08908`](https://github.com/bondiano/flick-lang/commit/35d08908), [`1332abd2`](https://github.com/bondiano/flick-lang/commit/1332abd2), [`5a554f0f`](https://github.com/bondiano/flick-lang/commit/5a554f0f), [`0b8563d1`](https://github.com/bondiano/flick-lang/commit/0b8563d1), [`00f7d320`](https://github.com/bondiano/flick-lang/commit/00f7d320), [`70275715`](https://github.com/bondiano/flick-lang/commit/70275715) |
| **2026-11-07–08: ordinary APIs and richer extension/editor boundaries** | `provide layer` becomes `Layer.run`; `scope` becomes rank-2 `Scope.run`. Plugin hooks extend to file loading, output rewriting, and IR rewriting. The playground gains retained sessions, an editor integration, and sandboxed interface previews. | [`6fc93488`](https://github.com/bondiano/flick-lang/commit/6fc93488), [`fc4ae0b7`](https://github.com/bondiano/flick-lang/commit/fc4ae0b7), [`6ef752a5`](https://github.com/bondiano/flick-lang/commit/6ef752a5), [`f9d79008`](https://github.com/bondiano/flick-lang/commit/f9d79008), [`829b4211`](https://github.com/bondiano/flick-lang/commit/829b4211), [`f5a1c79f`](https://github.com/bondiano/flick-lang/commit/f5a1c79f), [`e2db2367`](https://github.com/bondiano/flick-lang/commit/e2db2367) |
| **2026-11-09–10: own runtime and broader modules** | Flick-owned execution is added, the Effect adapter becomes a separate package, and the project default changes to RT. Module contracts, parameterized modules, and inline modules arrive in the same interval. | [`9033a391`](https://github.com/bondiano/flick-lang/commit/9033a391), [`a1a932a0`](https://github.com/bondiano/flick-lang/commit/a1a932a0), [`313b3191`](https://github.com/bondiano/flick-lang/commit/313b3191), [`194924d5`](https://github.com/bondiano/flick-lang/commit/194924d5), [`7be863bd`](https://github.com/bondiano/flick-lang/commit/7be863bd) |

These are editorial groupings, not official release phases. The early `M0`–`M22` wording belongs to historical commit subjects; it should not be mistaken for today's implementation-status checklist.

## Long-lived entry points, not frozen implementations

Several current entry points date to the repository's first day. “Surviving” here means the file's history remains connected to that introduction, not that its contents, APIs, or algorithms stayed unchanged.

| Surviving capability / current path | First recorded commit, UTC date | Non-merge file-touch commits through `6e9096fb` |
| - | - | -: |
| Native CLI: `flick/src/main.rs` | `4e77f06a`, 2026-10-22 | 29 |
| Browser compiler: `flick-wasm/src/lib.rs` | `21f72977`, 2026-10-22 | 15 |
| Lossless syntax/formatter entry: `flick-core/src/syntax/mod.rs` | `e7327dfa`, 2026-10-22 | 12 |
| Module/name resolution: `flick-core/src/resolve/mod.rs` | `d37b3515`, 2026-10-22 | 14 |
| Type checker entry: `flick-core/src/types/mod.rs` | `d3db460e`, 2026-10-22 | 103 |
| Typed lowering entry: `flick-core/src/ir/mod.rs` | `9acb7da9`, 2026-10-22 | 29 |
| Code generation entry: `flick-core/src/codegen/mod.rs` | `b7f8761d`, 2026-10-22 | 19 |

Method: run `git log --follow --no-merges --format='%H %cI %s' 6e9096fb -- <one-path>` separately for each path, count returned commits including the introducing commit, and inspect the oldest entry. These are **file-touch counts**, not counts of feature rewrites or subsystem-wide changes. Moving work into sibling files can leave an entry point untouched. Uncommitted changes are excluded.

The browser entry point is particularly early: it was introduced roughly seven minutes after the initial CLI/core workspace, according to their recorded committer timestamps. That chronology supports “shared native/browser compiler from the beginning”; it does not imply feature parity at every historical revision.

## Replacement and extraction, not just addition

### Ref-driven roots gave way to atoms

On October 25, [`5daf1438`](https://github.com/bondiano/flick-lang/commit/5daf1438) explicitly removes the Ref-driven UI API and moves every root onto atoms: 50 changed files, 921 added lines, and 1,184 deleted lines. Its diff includes library declarations, renderer code, documentation widgets/playground code, and tests.

[`57a7b63b`](https://github.com/bondiano/flick-lang/commit/57a7b63b), later that day, adds rendering templates to HTML without React. This is evidence for a separate rendering path, **not** evidence that React support disappeared. Current `runtime/src/ui/` and `packages/effect-ts/src/ui/` retain distinct integration surfaces. See [runtimes](packages/runtimes/index.md).

### Builtins and DSLs became package/plugin code

[`35d08908`](https://github.com/bondiano/flick-lang/commit/35d08908), November 3, moves builtin modules into the current `packages/core/src/` and `packages/std/src/` layout. Its rename-aware summary is 227 changed files, 254 insertions, and 365 deletions: substantial relocation without comparable net growth.

The following commits move behavior across boundaries rather than removing the language features:

- November 4: [`5a554f0f`](https://github.com/bondiano/flick-lang/commit/5a554f0f) moves CLI blocks out of compiler-specific DSL handling; current implementation lives in `plugins/cli/`.
- November 5: [`0b8563d1`](https://github.com/bondiano/flick-lang/commit/0b8563d1) moves `server fn` out of dedicated checker/lowering handling into the std plugin; see `plugins/derive/src/`.
- November 6: [`00f7d320`](https://github.com/bondiano/flick-lang/commit/00f7d320) introduces SQL-table expansion as the SQL plugin; see `plugins/sql/`.

The current linkage is explicit in `flick-core/src/plugin/first_party.rs`: std supplies the `cli`, `server`, `codec`, `sql`, and `component` heads. That source is stronger evidence of today's ownership than older references to special compiler DSL passes. See [plugins](packages/plugins.md).

### Shared formatting and IR were extracted

October 30 introduces `flick-pretty/` in [`edf97f35`](https://github.com/bondiano/flick-lang/commit/edf97f35). [`08a4c13a`](https://github.com/bondiano/flick-lang/commit/08a4c13a) then moves canonical formatting onto its document representation; [`bfc7fe71`](https://github.com/bondiano/flick-lang/commit/bfc7fe71) does the same for Go generation. `flick-pretty/src/lib.rs` still documents the arena-based printer.

November 6 extracts IR data into `flick-ir/`: [`a6e2c354`](https://github.com/bondiano/flick-lang/commit/a6e2c354) starts the crate, and [`70275715`](https://github.com/bondiano/flick-lang/commit/70275715) makes the IR data live there. Today `flick-core/src/ir/mod.rs` owns lowering, while `flick-ir/src/lib.rs` exports the shared representation. “IR moved” does not mean the compiler's lowering pipeline was replaced wholesale.

### Surface syntax became ordinary library operations

November 7 records two explicit breaking transitions: [`6fc93488`](https://github.com/bondiano/flick-lang/commit/6fc93488) changes `provide layer` into `Layer.run`, and [`fc4ae0b7`](https://github.com/bondiano/flick-lang/commit/fc4ae0b7) changes `scope` into rank-2 `Scope.run`. The current declarations are in `packages/std/src/Layer.fl` and `packages/std/src/Scope.fl`; matching model/proof work appears in nearby commits.

This history explains why some old comments still use the earlier vocabulary. It does not establish compiler/runtime correctness: the reference model and Lean proofs have their own stated limits in `formal/README.md` and the [formal-model page](systems/formal-model.md).

### Vendoring caused a spike, then was reversed

On October 31, [`69211888`](https://github.com/bondiano/flick-lang/commit/69211888) vendors pgx for PostgreSQL: 160,162 lines added with rename detection disabled. [`31451423`](https://github.com/bondiano/flick-lang/commit/31451423) switches to downloading pgx on demand less than two hours later and deletes 160,152 lines under the same measurement.

That pair alone demonstrates why historical added/deleted lines are not language growth or engineering productivity. The current source inventory excludes vendor trees; `runtime-go/go.mod` is the current dependency boundary.

### Effect stopped being the only application default

[`9033a391`](https://github.com/bondiano/flick-lang/commit/9033a391), November 9, adds Flick-owned execution. [`a1a932a0`](https://github.com/bondiano/flick-lang/commit/a1a932a0) makes RT standalone and splits out `packages/effect-ts/`; with rename detection disabled, the split produces 29,172 added and 28,701 deleted lines. That near-balanced churn is not evidence of a 29,172-line net rewrite.

[`194924d5`](https://github.com/bondiano/flick-lang/commit/194924d5) changes `Build::default()` from `Backend::Effect` to `Backend::Rt`. Its committer timestamp is November 9 at 21:43:59 UTC, or November 10 at 04:43:59 in the commit's `+07:00` offset. The current assignment is visible in `flick-core/src/project/config.rs`.

Current `runtime/package.json` describes the Flick-owned runtime; `packages/effect-ts/package.json` describes the separate adapter and exports its bridge. The supported Effect integration remains, so “the Effect adapter was deleted” would be an incorrect reading of this transition.

## Growth at actual revisions

These are historical Git-tree measurements, not estimates from churn. Count regular source blobs and sum their uncompressed blob sizes using `git ls-tree -rl --full-tree <revision>`, applying the source extensions and path/generated-output exclusions documented in [the inventory method](by-the-numbers.md#inventory-method-and-exclusions).

| UTC date / point | Revision | Retained source files | Source bytes |
| - | - | -: | -: |
| 2026-10-22, initial workspace | `4e77f06a` | 2 | 746 |
| 2026-10-22, standard library/test runner milestone | `e3e9d6e2` | 303 | 1,169,856 |
| 2026-10-31, reference checker/interpreter introduction | `cf94115a` | 1,803 | 8,371,853 |
| 2026-11-06, shared-IR branch merged | `cd26f200` | 2,527 | 12,946,267 |
| 2026-11-10, committed snapshot | `6e9096fb` | 2,929 | 14,850,573 |
| 2026-11-10, measured dirty working tree | not a Git revision | 2,931 | 14,867,419 |

Each revision includes only its ancestors, not every other branch whose timestamp precedes it. These samples are not release tags. Historical exclusions use the same explicit path rules, not a claim to detect every generated file in every past layout.

## Key source files

| Path | Present-day connection to the history |
| - | - |
| `flick/src/main.rs`, `flick-wasm/src/lib.rs` | Native/browser entry points traceable to the first day |
| `flick-core/src/syntax/mod.rs`, `flick-core/src/types/mod.rs` | Long-lived syntax/checker boundaries |
| `packages/core/src/Prelude.fl`, `packages/std/src/Scope.fl`, `packages/std/src/Layer.fl` | Library ownership and the evolved effect/region APIs |
| `flick-core/src/plugin/first_party.rs` | Current first-party DSL/plugin ownership |
| `flick-pretty/src/lib.rs`, `flick-ir/src/lib.rs` | Extracted shared infrastructure |
| `flick-core/src/project/config.rs` | Current RT default, rather than an inference from old comments |
| `runtime/package.json`, `packages/effect-ts/package.json`, `runtime-go/go.mod` | Current execution/dependency package boundaries |

No individual contributor rankings were used. Git timestamps, file-touch counts, and commit subjects describe the recorded history; they neither measure effort nor certify that the current dirty tree passes its executable checks.
