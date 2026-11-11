# Debugging runbook

Active contributors: bondiano, Vassiliy Kuzenkov — nonbot author names recorded on `origin/main` for the toolchain, host, and compiler/editor boundaries.

Begin with the failing input, selected project, compiler/runtime versions, and exact diagnostic or output difference. The diagnostic registry in `flick-core/src/diagnostics/codes.rs`, command adapters in `flick/src/commands/`, and the actual test harnesses are the primary evidence.

## Preserve a reproducible context

Record the branch/commit and relevant dirty diff; executable version; operating system; selected project/member; source, manifest, lockfiles, and plugin component identity; backend, emit target, and profile; and the exact command, exit status, stdout, and stderr. Redact secrets and private source before sharing logs or snapshots.

For direct commands against this checkout, set `FLICK_STD_PATH` to its absolute `packages/std` directory. The `justfile` already does this. A released compiler may otherwise resolve a version-tagged implicit package, while a recognized checkout may use local sources.

The following examples run **from the failing project's root**, using the matching compiler on `PATH`; substitute the real module name:

```sh
flick --version
flick doctor
flick check --diagnostics json
flick explain FLICK-T012
```

`doctor` checks Node's minimum, the project's exact pnpm pin, installed runtime/adapter versions, and other tool information. Optional Go/adapters/Content Mapper observations can be notes rather than failures; a note is not proof that a later build will succeed. Its implementation is `flick/src/commands/tools.rs`, not a separate doctor module.

## Diagnostic triage

1. Reproduce through the reported entry point before reducing to a lower layer.
2. Explain the reported code with `flick explain <code>`. Package/plugin codes use `flick explain <package>/<code>` and the package's explanation files.
3. Retain the source span, severity, help, and any safe-fix edits. Check whether `--warnings-as-errors` changed the result.
4. For machine output, keep stderr separate from stdout. Generic diagnostics/service events belong to stderr; command data and application stdout remain separate.
5. Reduce unrelated code while retaining imports, project ownership, effect/region context, and expansion origins needed to trigger the problem.
6. Add an independently justified regression before the fix.

`flick/src/report.rs` uses generic command exits 0 for success, 1 for ordinary errors, 2 for its listed usage/configuration/dependency errors, and 3 for P003 internal-tooling failures. Execution commands forward child outcomes; do not interpret every application exit using the generic table.

`check --fix` writes selected source files, accepts only safe nonoverlapping whole fixes, and repeats at most ten passes. It is a mutation tool, not the first diagnostic collection step. Review its diff and do not use it to bulk-rewrite span-sensitive checker fixtures.

## Locate the first wrong representation

```sh
flick inspect App.Main --emit ast
flick inspect App.Main --emit typed-ir
flick inspect App.Main --emit iface
flick inspect App.Main --emit js
flick inspect App.Main --emit ts
flick inspect App.Main --emit mapping
```

The valid representations are defined in `flick/src/args/tools.rs`; implementation lives in `flick/src/commands/inspect.rs`. Inspection first compiles/checks the project, so it is not a way around an existing project/type error. Its AST form prints the lossless syntax tree; typed IR, interfaces, emitted code, and projection mappings answer different questions.

| Symptom | First boundary to examine | Regression home |
| - | - | - |
| Unexpected parsing/recovery or formatting | Source → lossless tree → syntax AST; formatter AST/idempotence checks | `tests/syntax/`, `flick-core/tests/corpus.rs`, parser/fmt units |
| Wrong name, type/effect/region rejection, or fix | Resolver/checker and diagnostic origin before lowering | `tests/check/fail/`, nearby pass case, project fixture where needed |
| Correct checking, wrong result/order/profile | Typed IR/stages → optimization → chosen backend → runtime | `tests/run/`, `tests/language/`, relevant lowering/runtime tests |
| Dependency/module missing only in a project | Discovery, import roots, manifest/lock, installed source packages | `tests/check/projects/`, `flick/tests/source_packages.rs`, project integration |
| Old output after source/config change | The particular cache and its input inventory | Build/package/plugin cache regression with reuse and invalidation |
| Editor only, or after an edit | In-memory document text, UTF-16 conversion, project context, stale-result handling | IDE/LSP open/edit/query/close tests |

See [syntax/checking](../systems/compiler/index.md), [IR and optimization](../systems/compiler/typed-ir-and-optimization.md), and [diagnostic formatting](../systems/compiler/formatting-and-diagnostics.md).

## Project, package, and cache failures

Check the selected manifest first: nearest-project discovery, explicit `--project`, workspace `--package`, source/test roots, dependency kinds, and std selection can change the graph. Module names follow paths; source modules cannot import ordinary test-root modules.

`flick build --inputs` prints the conservative input inventory/key without building artifacts. Compare its roots, modules, configuration, and component/file inputs before deleting anything. It is not a trace of the exact dependency reads of a completed compilation.

`flick install --locked` refuses lock updates; offline mode requires warm stores/snapshots. A missing locked dependency needs installation, not a checker workaround. Do not edit fetched-store sources as though they were path dependencies.

Separate these reuse layers:

- Project artifact/test cache under `.flick/`.
- Shared builtin, checked-package, and plugin-call caches under the user cache.
- Fetched source-package store, optionally relocated with `FLICK_STORE`.
- In-memory editor module/parse cache.

`flick clean` removes manifest-owned outputs, test output, and unused generations. `flick clean --cache` additionally clears the project build cache; it does **not** purge every shared cache or the fetched store. Capture the failing key/input evidence first, and do not use broad deletion as a diagnosis.

For details and current limitations, use [caches and ownership](../systems/project-and-cli/caches-and-artifacts.md) and [project loading](../systems/project-and-cli/project-loading-and-packages.md).

## Plugin and foreign-interface failures

```sh
flick plugins
flick expand src/App/Main.fl
flick ffi check
```

For plugins, distinguish component absence, incompatible SDK/WIT version, a CLI built without `wasm-plugins`, grant denial P038, offline snapshot failure P039, fuel/memory traps, generated-code errors, and invalid hooks. `FLICK_PLUGIN_TRACE` can append call traces; use a fresh chosen path and review for sensitive data before sharing.

The LSP requests offline plugin HTTP behavior. CLI `--offline` or the presence of `CI` also selects snapshot-only plugin HTTP. Granted file/environment reads still work; these flags do not sandbox pnpm, Go, scripts, or every implicit-package fetch. Inspect `.flick/plugin-snapshots` carefully: response bodies may be sensitive.

For FFI, confirm installed declarations, helper selection (`FLICK_DTS_IMPORTER` when overridden), adjacent policy, import provenance, and reviewed export fingerprints. `flick ffi check` diagnoses generated-text or reviewed-shape drift; it does not prove a `pure`, cancellation, or callback-retention assertion true. Fix policy or reviewed bindings deliberately rather than editing generated output to suppress drift.

Use the [plugin host](../systems/project-and-cli/plugin-host.md) and [foreign interface](../systems/project-and-cli/foreign-interface.md) pages for precise boundaries.

## Runtime, watch, editor, and browser failures

- **Execution:** retain stdout/stderr/exit and trace evidence; compare debug/release and the supported target independently against expected behavior. Use focused `FLICK_E2E_ONLY` cases, `just language`, and applicable model/conformance checks. Backend agreement alone can preserve a shared bug.
- **Dev/watch:** a failed replacement build should leave the previous application running. Successful dev replacement waits for termination/finalization before starting the next generation. Test watch waits for the previous test runner to release resources. Inspect immutable `.flick/run/` generation ownership before attributing a late import to stale output.
- **LSP:** verify the editor launches the intended `flick lsp --stdio` binary and project. Reproduce open → edit → query → close, especially UTF-16/non-ASCII positions. Do not print ad-hoc text into the stdio protocol.
- **Browser:** distinguish WASM initialization, the persistent compiler queue, the disposable program worker, and sandboxed UI preview. Canceling queued compiler work prevents dispatch; active synchronous WASM is not interrupted, and its stale result is discarded. Stopping a program worker is a different operation.
- **Site integration:** rebuild/restart the native compiler as described in `CONTRIBUTING.md` when debugging Astro compilation. Search needs a production build/preview because Pagefind's index is generated during the build.

For runtime issues use [runtime architecture](../packages/runtimes/index.md); for stale editor/browser state use [language server](../systems/project-and-cli/language-server-and-ide.md) and [browser tools](../systems/browser-tools.md).

## Close the investigation

Keep the original input, independent expectation, failing regression, first wrong representation, fix, and exact check outcomes together. Review any snapshot/fingerprint changes rather than accepting observed output automatically. If a tool/service is unavailable, report that exact blocker and required follow-up.

This is a source-based runbook for the dirty `main` snapshot based at `6e9096fb`, not evidence that the commands above were executed during wiki generation.

## Key source files

| Source | Responsibility |
| - | - |
| `flick/src/commands/tools.rs`, `flick/src/report.rs` | Doctor/explanations and generic exits |
| `flick/src/commands/inspect.rs`, `flick/src/args/tools.rs` | Available representations and their compilation path |
| `flick/src/commands/inputs.rs`, `flick/src/cache.rs` | Input inventory, keys, artifact ownership |
| `flick/src/commands/check.rs` | Selection and safe-fix behavior |
| `flick/src/plugins/mod.rs`, `flick/src/plugins/outside.rs` | Tracing, component selection, snapshots |
| `flick/src/commands/run.rs`, `flick/src/commands/dev.rs` | Generation and replacement lifecycles |
| `docs/src/playground/compiler.ts`, `docs/src/playground/compiler.worker.ts` | Browser queue and execution handoff |

Next: choose the regression and required gates in [testing](testing.md).
