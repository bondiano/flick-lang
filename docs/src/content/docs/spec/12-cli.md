---
title: "12. The Flick CLI"
---

The CLI is called `flick`. The interface below is fixed for implementing the first version of Flick; a command's presence in the specification does not mean it is already implemented. All commands use a single Rust core and the chosen Effect backend.

## File names and project configuration

| Object | Convention |
| - | - |
| Language and tools | Flick; in Russian, «флик»; the executable is `flick` |
| Source file | `.fl`, for example `src/App/Main.fl` for the module `App.Main` |
| Compiler configuration | `flick.toml`; the configuration schema has its own version |
| Package and dependencies | `package.json`; the first supported package manager is pnpm |
| Pinned dependency tree | One `pnpm-lock.yaml` per project or pnpm workspace |
| Native interface | Versioned JSON `.iface`; preserves the full Flick types and ABI |
| Public interface for TS | `.d.ts`, if requested and the chosen façade is representable |
| Build artifacts | By default `dist/debug/` and `dist/release/` |
| Local cache and service files | `.flick/`; the cache is separate from the sources and the lockfile |

One `.fl` file declares one module. For ordinary sources, the path inside the source root corresponds to the module name: `App.Users` is placed in `src/App/Users.fl`. The test root forms a separate set of modules with the same rules. A collision of a full module name is diagnosed. The only source extension in the first version is `.fl`.

```toml
schema-version = 1

[project]
kind = "app"
source-dir = "src"
test-dir = "test"
entry = "App.Main"

[build]
out-dir = "dist"
profile = "debug"
emit = "js"
emit-dts = false
schema-aot = false
```

`kind` accepts `app` or `lib`; a library does not need `entry`. `entry` names the module with the exported `main`. Package names, their versions and dependencies are stored in `package.json`, Flick settings in `flick.toml`. A command looks for the nearest `flick.toml` from the working directory upward; `--project <path>` explicitly selects the project directory or the configuration file.

## Everyday commands

| Command | Purpose and contract |
| - | - |
| `flick new <directory> --template app` | Create a new directory with the configuration, `package.json` and an example application. `--template cli` creates a command-line application whose `main` is a `cli` block, `--template lib` a library. |
| `flick init --template app` | Add the same skeleton to the current directory; existing files are not overwritten, and a conflict is shown before writing. |
| `flick check [paths...]` | Check syntax, kinds, types, effects, instances, captures, imports and `.iface`. Without arguments it checks the project's sources and tests. |
| `flick build` | Check the project and emit ESM, source maps and native interfaces into the directory of the selected profile. |
| `flick run -- [args...]` | Build JS and run the `main` from the configuration; pass the arguments after `--` to the application. `--entry App.Main` explicitly selects the entry module. |
| `flick dev -- [args...]` | Watch the sources, rebuild the application and sequentially replace the running version after a correct shutdown. |
| `flick test [paths...]` | Build and run the tests from the test root; each test gets its own scoped owner. |
| `flick fmt [paths...]` | Format Flick sources. `--check` checks the format without writing; `--stdin --stdin-filepath <file>` serves the editor. |
| `flick clean` | Remove only the recorded artifacts of this project; `--cache` additionally removes its Flick cache. |

By default `new` and `init` only create files; installation is performed by the separate `install` command. `check`, `build` and the build stages of `run`/`dev`/`test` do not install dependencies, do not run `main` and do not perform effectful initialization of foreign modules. Ordinary SQL checking does not connect to a database. A missing dependency produces a diagnostic with installation instructions.

The choice of `paths` sets the roots of checking, but does not disable checking of their transitive imports, coherence and ABI. `check --watch` only rechecks code; `build --watch` updates artifacts; starting and restarting the application is managed by `dev`.

## Dependencies and packaging

Downloading packages and choosing their versions are delegated to pnpm and the npm ecosystem. Flick checks typed imports, `.iface`, canonical instances and the runtime ABI on top of the installed tree. Flick has no separate version solver, registry or second dependency lockfile. The initial adapter supports pnpm; adapters for other package managers can be added later without changing the module system.

| Command | Purpose |
| - | - |
| `flick install` | Install dependencies through the pinned pnpm and check toolchain/runtime compatibility. |
| `flick install --locked` | Install strictly according to the existing lockfile; a missing lockfile or a mismatch with the manifest is an error. |
| `flick install --locked --offline` | The same installation from the local store only; a missing package is an error. |
| `flick add <package-spec> [--dev]` | Add an npm-compatible package to dependencies or devDependencies and update the lockfile. Importing its JS API still requires FFI. |
| `flick remove <package>` | Remove a direct dependency through the package manager; the remaining source imports are then diagnosed by the checker. |
| `flick update [package]` | Update the resolved versions within the manifest constraints. The pinned compiler/runtime/Effect versions are not changed implicitly. |
| `flick deps [--why <package>]` | Show the installed graph, the reasons a dependency appeared and information about the available Flick interfaces. |
| `flick pack --out-dir artifacts` | Build a release package and a local npm-compatible tarball with ESM, `.iface` and the declared TS façades. |

`install --locked` maps to `pnpm install --frozen-lockfile`, and `--offline` to pnpm's flag of the same name. These modes and their behavior are described in [pnpm install](https://pnpm.io/cli/install). The manifest and dependencies use the ordinary [package.json](https://pnpm.io/package_json); adding packages is delegated to [pnpm add](https://pnpm.io/cli/add).

The project template pins specific versions of the compiler, the runtime adapter and Effect, as well as the pnpm version through `packageManager`. The commands check that the local toolchain matches the runtime capability matrix from section 11; an incompatible ABI is not bypassed through a TS projection. Before changing the manifest, the lockfile or the installed tree, the adapter checks the actual version of the pnpm it calls. If the pinned version is unavailable, the command ends with code `2` and installation instructions; an arbitrary available version is not substituted. Extending the supported Effect versions requires checking the contracts, not just changing the semver range.

`pack` checks the completeness of the declared exports, `.iface`, dependency fingerprints and package files. Importing a packed library does not run the application. Publishing the tarball is done by the ordinary package manager as a separate action.

## Build and optimizations

`build`, `run`, `dev` and `test` accept `--profile debug` or `--profile release`; by default the profile from the configuration is used. `debug` is a conservative reference lowering. `release` enables the allowed typed IR passes from section 8 and preserves the same Effect runtime and observable contracts.

| Build parameter | Meaning |
| - | - |
| `--emit js` | Executable ESM modules; the backend's standard output. |
| `--emit ts` | A TS representation of the executable code for further JS tooling; this is not the editor's virtual projection. |
| `--emit-dts` | A checked public TS façade. A non-representable export requires an explicit wrapper or a change of the chosen façade; there is no silent `any`. |
| `--schema-aot` | Enable the chosen AOT optimization of Schema; an unsupported node may keep interpreter semantics. |
| `--out-dir <directory>` | Change the base directory of artifacts; profiles are kept in separate subdirectories. |
| `--watch` | Rebuild on changes, keeping the last successfully published generation on error. |

`run`, `dev` and `test` always build executable JS, regardless of the `emit` setting and the `build --emit ts` output mode. The first supported host for these commands is Node.js. Browser/React code is emitted as ESM and connected to a JS host and bundler; this CLI introduces neither a separate Rust/WASM backend nor automatic launching of a browser application.

The cache accounts for the sources, the compiler and language version, the normalized effective project and compiler configuration, including the source/test roots and `schema-aot`, the profile and emit flags, the stdlib/runtime versions, the lockfile, `.iface` and dependency fingerprints, FFI contracts and the versions of the TS tools involved. The artifacts of a new generation are declared ready only after the whole set has been built successfully. A running generation uses an immutable set of executable modules and allowed dependencies, for example in `.flick/run/<generation>/`; a late import must not see the files of the next build. This set is kept until shutdown completes. Parallel replacement of the dependency tree is permitted only if such a snapshot is preserved; otherwise the change is rejected before writing, with a diagnostic about the busy tree. `clean` works from the manifest of Flick artifacts, preserves active generations and their locks, and does not delete user files in a shared output directory.

## Running and safe restart

`run <name>` starts the selected entry through one external platform
runner; the name may be omitted when there is only one entry. A pure
`main : Unit -> Unit` is called directly. A runtime entry takes and returns
`Unit`, requires no services and may fail only with `AppError`; dependencies
and layers are provided inside `main`. The CLI does not turn each direct
call or `await` into a separate runtime launch. Command-line arguments are
available through an explicit platform API. In a runtime entry, a normal
return, typed `AppError`, defect or interruption closes the root owner;
the command's result is known only after cleanup. The test runner follows
the same cleanup order for each test.

`dev` can check and build the next version while the current one is running, preserving the isolation of generations described above. If the build fails, the current version keeps running. After a successful build, the runner closes admission of the old version, requests interruption, waits for the join of child fibers and the quiescence of registered foreign work, then completes the LIFO finalizers. The new version starts only after shutdown is confirmed complete; a cleanup error stops the automatic restart.

SIGINT/SIGTERM start the same termination protocol. Until cleanup is complete, the command does not report a successful exit; if there is a delay, it shows the shutdown state. This is not a hard deadline: uninterruptible work can delay stopping. Forced termination of the process by the OS is not evidence that finalizers ran. Automatic in-process hot reload that keeps old scoped handles is not part of the first version.

## Tests and reproducibility

`test` supports `--filter <text>` against the full test name, `--jobs <positive-number>` for an explicit parallelism limit, `--watch`, and `--reporter human` or `--reporter json`. By default `--jobs 1`. A repeated test run begins after the owners of the previous run are closed; cancelling a test does not permit skipping cleanup.

The test library supports pure and effectful tests, as well as separate property tests with explicit generators. `--seed <integer>` sets the seed for generators and is printed on failure; it does not promise to reproduce the fiber schedule, the network or an external database. Assertions, typed failures, defects and cleanup failures are reflected in a test's result separately.

Ordinary CI checks the application in the selected profile. For the Flick compiler/runtime, the semantic regression suites are run in `debug` and `release` to check that the established observable contracts are preserved; two successful runs are not considered general proof of equivalence of all programs.

## FFI, Schema, documentation and the editor

| Command | Contract |
| - | - |
| `flick ffi import <package> --out <directory>` | Read the installed `.d.ts`, generate the supported bindings and a coverage report; `--select <names>` restricts the surface. |
| `flick ffi check` | Check the generated bindings, separately stored reviewed overrides, fingerprints and the mandatory adapter policies after dependency changes. |
| `flick schema export <Module.codec> --target json-schema-2020-12 --out <file>` | Strictly export a statically representable JSON wire codec, with a node/path diagnostic for an unsupported contract. |
| `flick schema export <Module.api> --target openapi-3.1 --out <file>` | Strictly export OpenAPI 3.1 for an `HttpApi` value, with an endpoint/path diagnostic for an unsupported schema. |
| `flick doc --out-dir docs` | Produce documentation of the public API: Flick types, kinds, constraints, effects, captures and the signatures of exported values. |
| `flick lsp --stdio` | Start the source language server with the same Rust core and project configuration. |
| `flick explain <diagnostic-code>` | Explain a Flick diagnostic and show small examples of fixes. |
| `flick doctor` | Check the compiler, Node.js, pnpm, the runtime/Effect ABI and the state of the connected TS/editor adapters. |
| `flick inspect <module> --emit <representation>` | Print `ast`, `typed-ir`, `js`, `ts`, `iface` or `mapping` for investigating codegen and source maps. |

`ffi import` separates supported exports, exports with a missing policy and unsupported exports. The mandatory rules for purity, execution, retention, cancellation and ownership are not inferred automatically from `.d.ts`. Reviewed overrides are not overwritten by regeneration. Blocked exports stay named; requesting or using one yields an error, while independent supported exports are preserved.

`schema export` uses the strict wire contract defined in section 9. The command does not execute the application's main or arbitrary user runtime transformations for the sake of the export. Dynamically built schemas are exported through an explicit runtime API. Standard Schema V1 and Arbitrary remain library adapters/generators; they are not passed off as static JSON files. OpenAPI 3.1 is exported from a statically known `HttpApi` value under the same strict contract (`--target openapi-3.1`): the document contains no `{}`/`any`, and an unsupported schema of an endpoint yields an ExportError with a path. Swagger UI and Scalar are library Layers that serve this same document, rather than building their own approximation.

`lsp` serves source diagnostics, hover, definition, completion, rename and formatting. Its stdout belongs entirely to the protocol; logs go to stderr. The Content Mapper remains a separate versioned integration and is not a condition for the source checker to work. The absence of an optional Mapper adapter in `doctor` does not by itself block compilation.

## Common options and diagnostics

| Option | Purpose |
| - | - |
| `--project <path>` | Explicitly select the project. |
| `--diagnostics human` or `--diagnostics json` | The format of diagnostics on stderr; JSON uses an NDJSON stream with a schema version. |
| `--warnings-as-errors` | Make warnings errors for the command's result. |
| `--color auto`, `--color always`, `--color never` | Control the styling of human-readable output. |
| `--quiet` or `--verbose` | Reduce service output or enable detailed information. |
| `--help` and `flick help <command>` | Help for the CLI or a specific command. |
| `--version` | The Flick compiler version; detailed compatibility is available through `doctor`. |

A diagnostic has a stable code with the prefix `FLICK-`, a severity, a message, a source path, a range of UTF-8 bytes, related spans and possible fixes. The range refers to the `.fl` source if the correspondence is known; an error in external JS/TS cannot be attributed to an arbitrary source span. The LSP adapter translates the range into the editor's position encoding.

stdout is intended for the command's result, the application's stdout or the selected protocol; service messages and diagnostics do not interfere with this stream. With `--diagnostics json`, all of stderr belongs to a single serializer of versioned NDJSON: compiler diagnostics, service logs, application stderr and package manager output become typed events `diagnostic`, `log`, `application-stderr`, `package-manager-output`. For captured output, the original channel and text are preserved; raw lines of child processes and ANSI codes are not mixed into this stream. In human mode, the application's stderr may be passed through directly. `test --reporter json` separately emits structured test events on stdout and includes captured test logs as event fields. The LSP's stdout always belongs only to its protocol.

| Exit code | Meaning |
| - | - |
| `0` | Success; for `fmt --check`, the formatting already conforms to the rules. |
| `1` | A check/build error, failed tests, a formatting mismatch, or an unsuccessful application/cleanup run. |
| `2` | An invalid invocation, configuration, a missing dependency, a busy dependency tree or an incompatible toolchain/ABI. |
| `3` | An internal error of the Flick compiler/tooling. |
| `130` / `143` | SIGINT / SIGTERM, if the only cause of the stop was interruption by the signal and cleanup completed successfully. |

An application or cleanup error keeps code `1` and the diagnostic of the final `Cause`, even if a signal arrived at the same time. Until cleanup is complete, the command does not report successful completion or a final exit code.

## Usage examples

```sh
flick new my-app --template app
cd my-app
flick install
flick check
flick dev -- --port 3000
```

```sh
flick fmt --check
flick check --diagnostics json
flick test --filter Users --jobs 4
flick build --profile release
flick run --profile release -- --port 3000
```

```sh
flick add date-fns
flick ffi import date-fns --out src/Foreign/DateFns
flick ffi check
flick schema export App.Domain.user_json --target json-schema-2020-12 --out schema/user.schema.json
flick doc --out-dir docs
```

The `date-fns` example shows a sequence of actions, not a promised full coverage of its API. After the import, the developer fills in the required reviewed adapter policies; `ffi check` verifies them. `App.Domain.user_json` in the example is an explicitly declared JSON codec of the project.

```sh
flick install --locked
flick fmt --check
flick check --warnings-as-errors
flick test --profile release
flick build --profile release
```

The last set is a baseline CI. A cache miss does not change the command's result; the lockfile and toolchain pins are part of the cache key. Updating dependencies is done separately from the CI check.

## Order of command implementation

| Stage | Contents |
| - | - |
| First executable slice | `new`, `init`, `check`, `build`, `run`, `fmt`, help/version and a minimal `install`; one project and a Node.js host. |
| Development loop | `dev`, `test`, `clean`, the remaining dependency commands, `lsp`, `doctor`, `explain`; stable JSON diagnostics and an incremental cache. |
| Full toolset of the first version | `ffi import`, `ffi check`, `schema export`, `doc`, `pack`, `inspect`; the corresponding language features remain part of the chosen v1 design. |
| After the first version | REPL, a separate benchmark runner, coverage, live docs, additional package-manager adapters and automated React/bundler integration. |

The stages set the order of implementation, not alternative versions of the semantics. Public commands go through the shared project model and a single checker; optimization, FFI import and editor projection do not form independent sources of types.
