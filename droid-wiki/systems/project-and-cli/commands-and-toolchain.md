# Commands and toolchain

Active contributors: bondiano, Vassiliy Kuzenkov

`flick/src/main.rs` prepares implicit packages and compiler interfaces, selects workspace members, then dispatches commands. Commands share diagnostics and process adapters, but build, run, test, package installation, and editor serving have distinct lifecycles.

## Command map

The authoritative surface is `flick/src/args.rs` and `flick/src/args/tools.rs`, not this abbreviated table.

| Commands | Behavior | Primary sources |
| - | - | - |
| `check`, `fmt` | Project/file diagnostics, safe fixes, formatting, stdin formatting, Markdown examples | `flick/src/commands/check.rs`, `flick/src/commands/check/pages.rs`, `flick/src/commands/fmt.rs` |
| `build` | JS/TS/Go output, declarations, optional IR export, input inventory, watch mode | `flick/src/commands/build.rs`, `flick/src/commands/emit_ir.rs`, `flick/src/commands/inputs.rs` |
| `test` | Compile suites, select/filter tests, seed generators, jobs, human/JSON results | `flick/src/commands/test.rs` |
| `new`, `init` | App, CLI, fullstack, library, or plugin templates; init keeps existing files | `flick/src/commands/scaffold.rs`, `flick/src/commands/templates/` |
| `run`, `dev` | Execute entries/scripts or rebuild and replace running generations | `flick/src/commands/run.rs`, `flick/src/commands/dev.rs`, `flick/src/commands/script.rs` |
| `clean` | Remove build-owned artifacts and unused generations | `flick/src/commands/clean.rs` |
| `install`, `add`, `remove`, `update`, `deps` | Flick source dependencies plus npm installation/query adapters | `flick/src/commands/deps.rs`, `flick/src/commands/libraries.rs`, `flick/src/fetch/mod.rs` |
| `pack`, `diff` | Source-library/app packaging and public-API comparison against Git | `flick/src/commands/pack.rs`, `flick/src/commands/tarball.rs`, `flick/src/commands/diff.rs` |
| `plugins`, `expand` | Inspect plugin origins/pins and expanded code | `flick/src/commands/plugins.rs`, `flick/src/commands/expand.rs` |
| `ffi import`, `ffi check` | Generate and review declaration-based bindings | `flick/src/commands/ffi.rs` |
| `inspect`, `doc`, `schema export` | Compiler views, API documentation, JSON Schema/OpenAPI output | `flick/src/commands/inspect.rs` |
| `doctor`, `explain` | Toolchain checks and diagnostic explanations | `flick/src/commands/tools.rs` |
| `lsp`, `content-mapper` | Editor and TypeScript stdio protocols | `flick/src/lsp/mod.rs`, `flick/src/mapper.rs` |

Global flags control human/JSON diagnostics, color, verbosity, project/member selection, warnings-as-errors, and offline behavior. `--project` and workspace `--package` conflict, in `flick/src/args.rs`.

## Diagnostics and exit behavior

`Command::run` returns an `Outcome` containing diagnostics plus their `SourceMap`, in `flick/src/command.rs`. Rendering and ordinary service messages go to stderr; stdout remains available for command data or the executed application. Human summaries are suppressed in JSON/quiet mode.

`flick/src/report.rs` uses exit 0 for success, 1 for ordinary errors, 2 for the listed usage/configuration/dependency diagnostics, and 3 for internal-tooling diagnostic P003. `--warnings-as-errors` promotes warnings before reporting and calculating the generic command result. Do not infer child application exit codes from this table: execution commands forward their child status through `flick/src/child.rs`.

`check --fix` applies only safe fixes to selected roots, never dependency files. Each overlapping multi-edit fix is accepted whole or skipped; checking/fixing repeats for at most ten passes, in `flick/src/commands/check.rs`.

## Build settings and compatibility

`flick/src/commands/build.rs` combines command-line overrides with project defaults, checks output/source overlap, rejects incompatible runtime options, attempts artifact reuse, compiles, runs build hooks, emits, and publishes. Errors before publication leave previous outputs alone; filesystem write failures are a separate limitation described under [artifact ownership](caches-and-artifacts.md).

Project defaults select Flick RT in `flick-core/src/project/config.rs`. File-only builds have no project configuration and follow the fallback Effect backend in `flick/src/commands/build.rs`. Go emission has its own backend path. Explicit RT mode with Go output or Promise façades is rejected; do not equate a project backend default with support for every emission combination.

`flick/src/toolchain.rs` currently records these compatibility values:

| Tool/package | Value or rule |
| - | - |
| Node.js | 24.18.0 minimum in `doctor` |
| pnpm | 12.16.1 fallback; a project's `packageManager` pin wins for dependency writes |
| Flick runtime | 0.1.0 |
| Effect adapter | 0.1.0, required by the Effect path rather than RT-only output |
| Effect | 4.1.2 for the adapter path |
| TypeScript Content Mapper verification target | 7.1.0-beta |

`check_runtime` rejects installed runtime/adapter version mismatches with P015; an absent package is skipped by that function, not certified as installed. `doctor` provides separate installed-tree checks in `flick/src/commands/tools.rs`.

Go builds use `go build -trimpath`, the local toolchain, module mode, and CGO disabled unless explicitly set, in `flick/src/toolchain.rs`. Optional runtime packages can require an already populated Go module cache or network access; `--offline` is not a universal process sandbox.

## Dependency commands

`flick/src/commands/deps.rs` fetches Flick sources and merges transitive JS requirements before invoking pnpm. It checks pnpm's actual version against the project pin and disables Corepack/pnpm version bootstrapping for that subprocess. This gate prevents the wrong pnpm from performing the npm write; it is not a transaction rolling back earlier Flick fetch/manifest steps.

`install --locked` maps to pnpm's frozen lockfile and refuses missing npm locks when an npm project is present. `install --offline` maps to the local package stores. Source-library additions use `--path`, `--git`/`--tag`, or `--url`/`--sha256`; ordinary `add` remains an npm operation. See `flick/src/args/tools.rs`, `flick-core/src/project/pnpm.rs`, and [package loading](project-loading-and-packages.md).

## Run and dev lifecycles

`flick/src/commands/run.rs` reserves a unique `.flick/run/<pid>-<n>/` generation and never overwrites it while running. Late dynamic imports therefore see the same build as the entry that started them. Locks and child PID records protect active generations from cleanup.

`flick/src/commands/dev.rs` builds a replacement before stopping the current app. A failed build keeps the running version. A successful build sends termination, waits for runtime shutdown/finalization, then starts the replacement; a shutdown failure stops restarts. With several unnamed entries, server entries run as processes and client pages use Vite. Client `run` uses the local Vite build/preview path in `flick/src/commands/run.rs`.

`flick/src/child.rs` forwards SIGINT/SIGTERM and drains relayed output before returning the child result. In JSON diagnostics mode, application stderr and tool output become typed events on stderr instead of corrupting the command's data stream.

Named `[scripts]` run shell steps in manifest order, with `@name` expanding another script and cycle detection, in `flick/src/commands/script.rs`. They execute trusted project shell commands, not restricted Flick expressions.

## Test and watch lifecycles

`flick/src/commands/test.rs` builds into `.flick/test/`, finds test-root modules exporting `tests`, then runs Node or one Go test executable. Filtering, jobs, seeds, and JSON reporting are runtime-runner inputs; project compilation still checks the selected project graph.

Build/check watches poll snapshots every 200 ms in `flick/src/main.rs`. Test watch closes the previous runner's stdin and waits for it to exit before starting the next run, in `flick/src/commands/test.rs`; dev has its separate generation replacement logic.

## Packaging

`flick/src/commands/pack.rs` requests a release build and rejects remaining `todo`/`echo` diagnostics. Libraries with package metadata produce source archives through `flick/src/commands/tarball.rs`; apps stage an npm package with entry runners and validate its artifact/interface completeness before packing. This is packaging, not automatic publishing.

## Modification and validation

Add a command in `flick/src/args.rs` or `flick/src/args/tools.rs`, wire dispatch in `flick/src/main.rs`, and implement it under `flick/src/commands/`. Keep machine-readable diagnostics separate from stdout and test help/flag conflicts as well as execution.

Regression entry points include `flick/tests/help_command.rs`, `flick/tests/cli_toolchain.rs`, `flick/tests/build_command.rs`, `flick/tests/test_command.rs`, `flick/tests/cli_run.rs`, `flick/tests/workflows.rs`, and `flick/tests/workspace.rs`. Run relevant CLI tests and `just e2e-tests` for executable changes. Wiki generation did not run these suites.

## Key source files

| Source | Responsibility |
| - | - |
| `flick/src/main.rs` | Startup and dispatch |
| `flick/src/args.rs`, `flick/src/args/tools.rs` | Flags, subcommands, templates |
| `flick/src/command.rs`, `flick/src/report.rs` | Outcomes, diagnostics, service events, generic exits |
| `flick/src/toolchain.rs` | Compatibility constants and Go compilation |
| `flick/src/commands/build.rs` | Build planning and publication |
| `flick/src/commands/run.rs`, `flick/src/commands/dev.rs` | Immutable generations and replacement |
| `flick/src/child.rs` | Signal forwarding and output relay |
| `flick/src/commands/deps.rs` | Native dependency workflow |

Related: [section overview](index.md), [editor protocols](language-server-and-ide.md), and [foreign bindings](foreign-interface.md).
