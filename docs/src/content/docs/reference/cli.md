---
title: "Command line"
---

`flick` is one binary over the compiler core. Every command takes the
global options `--project <path>`, `--diagnostics human|json`,
`--warnings-as-errors`, `--color auto|always|never`, `--quiet`/`--verbose`
and `--help`; `flick help <command>` prints a command's help and
`flick --version` the compiler's version (`flick doctor` shows the rest).

## Commands

| Command | What it does |
| - | - |
| `new <dir> [--template app\|cli\|fullstack\|lib]` | creates a project: `flick.toml`, `package.json`, an example module and its test; `cli` makes `main` a [`cli` block](../cli-dsl/); `fullstack` is a server entry and a browser page (`target = "client"`) sharing the calls of a [`server fn`](../server-fn/) module, writes guarded by `Server.authenticated`, with Vite for the page |
| `init [--template app\|cli\|fullstack\|lib]` | the same files in the working directory; every file that exists is reported before anything is written (`FLICK-P019`) |
| `check [paths] [--watch] [--fix]` | checks; without paths the source and test roots; `--fix` applies the fixes that keep the program's meaning (such as removing a type annotation the value already has, `FLICK-T021`) to those files and reports what is left |
| `build [paths] [--profile] [--labels all\|components\|none] [--emit js\|ts\|go] [--emit-dts] [--schema-aot] [--out-dir] [--watch]` | writes ESM, source maps and `.iface` files; then, unless `--quiet` or `--diagnostics json`, says on stderr how many modules it compiled and which runner starts each entry, or how many errors it found and where |
| `build [paths] --inputs` | prints a conservative JSON input inventory (`key`, `files`, `roots`, `modules`) without writing build artifacts or cache entries |
| `run [name] [--entry M] [--profile] [--devtools[=url]] -- args` | builds JavaScript and runs the `main` of the entry `name` (the only one when left out); `--devtools` sends its spans to the Effect DevTools ([observability](../observability/#devtools)) |
| `dev [name] [--entry M] [--profile] [--devtools[=url]] -- args` | rebuilds on changes and replaces the running app; in a project with several entries and none named, it runs every entry that is not a client's and, if one is, Vite (`node_modules/.bin/vite`) for the pages, which replaces a changed module in the page keeping the labelled atoms' values |
| `test [paths] [--filter] [--jobs] [--seed] [--reporter human\|json] [--watch] [--profile] [--emit js\|go]` | runs the tests of the test root, on Node or as a Go executable |
| `fmt [paths] [--check] [--stdin --stdin-filepath f]` | formats; without paths the source and test roots |
| `clean [--cache]` | removes the build artifacts; `--cache` also the build cache |
| `install [--locked] [--offline]` | installs with pnpm (`--frozen-lockfile`, `--offline`) |
| `add <pkg>… [--dev]`, `remove <pkg>…`, `update [pkg]…` | changes dependencies with pnpm |
| `deps [--why <pkg>]` | the installed graph, and the Flick modules of each direct dependency |
| `pack --out-dir <dir>` | a release build and its npm tarball; refuses `todo` and `echo` (`FLICK-P024`), and warns when the public API breaks importers since the latest tag |
| `diff [ref]` | the public API changes since a git revision (the latest tag by default), each `MAJOR` (removed or changed) or `MINOR` (added), and the version they call for from `package.json` |
| `ffi import`, `ffi check` | bindings from `.d.ts`, see [ffi.md](../ffi/) |
| `schema export <M.codec> --target json-schema-2020-12 --out <file>` | the strict JSON Schema of a codec |
| `doc --out-dir <dir>` | Markdown pages of the public API: types, kinds, constraints, effects, captures |
| `explain <code>` | the explanation of a diagnostic code |
| `doctor` | the compiler, Node.js, pnpm, the installed runtime and Effect, the optional adapters |
| `inspect <M> --emit ast\|typed-ir\|js\|ts\|iface\|mapping` | one representation of a module |

`check`, `build` and the build steps of `run`, `dev` and `test` install
nothing, run no `main` and load no foreign module. An installed
`@flick-lang/runtime` or `effect` of another version than the compiler
pins stops them with `FLICK-P015` (exit code 2). `schema export` loads the
module that makes the codec, whose top level is pure, and nothing else.

## Outputs and the cache

A build writes into `build.out-dir` (or `--out-dir`) and records the files
it wrote in `.flick/artifacts.json`. After successful publication, obsolete
manifest-owned files are removed, including after an emit-mode change or a
cache hit. `clean` removes the remaining owned files and empty directories;
other files in a shared output directory stay.

The cache in `.flick/cache/` keeps the artifacts of each build by a key over
the normalized selected roots separately from the available sources of the
project, its effective configuration, the profile, emit
and output settings, the compiler (version and binary), the installed
runtime and Effect, `package.json`, `pnpm-lock.yaml` and the manifests and
interfaces of the installed dependencies. Any change of these is a miss; a
hit writes exactly the files the build would.

Watch commands compare individual paths and their state, including file
creation and deletion; one unrelated timestamp cannot hide another edit.

## Running and replacing an app

`run` and `dev` always build JavaScript, whatever `emit` says. Each build
goes into an atomically reserved generation `.flick/run/<pid>-<n>/`, never
written again once its process starts, so a late `import()` sees the files
it started with. The CLI owns the generation from construction through
shutdown; `clean` preserves live construction reservations and running
children. The generation is removed after shutdown.

`dev` builds the next version while the current one runs. A failed build
leaves the current version running. A successful one first stops the
current version as SIGTERM does: the runner interrupts `main`, joins its
fibers and runs its finalizers. The new version starts only after that. A
shutdown that fails (any exit code but 143) stops the restarts with
`FLICK-P021`, and the runner's report of the cause stays on stderr.

SIGINT and SIGTERM reach the app; `flick` exits only after it, with its
exit code.

## Dependencies

Dependency commands delegate to pnpm. Every command that changes the
manifest, the lockfile or the tree first checks that `pnpm --version` is
the version `packageManager` pins (`FLICK-P016`, exit code 2, nothing
changed). `install --locked` without `pnpm-lock.yaml`, or with one out of
date, is `FLICK-P017` with exit code 2.

`pack` builds the project's modules in the release profile into a staging
package, with the runner of each entry of an app, which the staged
`package.json` names in `bin` (see [project.md](../project/#entry-points)), checks that every module has its `.iface` (and
`.d.ts` with `emit-dts`), that interfaces in the package agree on each
other's fingerprints and that the paths `exports` names exist
(`FLICK-P020`), and writes the tarball with `pnpm pack`. The staged
`package.json` declares `"flick": { "iface-dir": "dist" }`.

## Streams and exit codes

stdout carries only the result: the app's output, the test reporter, a
printed representation. Diagnostics and service messages go to stderr.
With `--diagnostics json` stderr is NDJSON, one event per line, every event
with `schema-version`: `diagnostic`, `log` (service messages),
`application-stderr` (the app's or the tests' stderr) and
`package-manager-output` (pnpm), the last three as
`{ "type", "channel", "text" }` without ANSI codes.

| Code | Meaning |
| - | - |
| `0` | success |
| `1` | errors in the code, failed tests, an unformatted file, a failed app or cleanup |
| `2` | a wrong invocation or configuration, a missing or wrong dependency or toolchain |
| `3` | an internal error of Flick |
| `130` / `143` | SIGINT / SIGTERM stopped the app and its cleanup succeeded |
