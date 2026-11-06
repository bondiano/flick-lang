---
title: "Command line"
---

`flick` is one binary over the compiler core. Every command takes the
global options `--project <path>`, `--package <name>` (`-p`, a member of the
[workspace](../project/#workspaces)), `--diagnostics human|json`,
`--warnings-as-errors`, `--color auto|always|never`, `--quiet` (alias `--silent`)/`--verbose`,
`--offline` (no network: [plugins](../plugins/#reaching-outside) answer
from the project's snapshots, which `CI` set means too, and `install`
installs from the store) and `--help`; `flick help <command>` prints a command's help and
`flick --version` the compiler's version (`flick doctor` shows the rest).

## Commands

| Command | What it does |
| - | - |
| `new <dir> [--template app\|cli\|fullstack\|lib\|plugin]` | creates a project: `flick.toml`, `package.json`, an example module and its test; `cli` makes `main` a [`cli` block](../cli-dsl/); `plugin` is a library with a [plugin](../plugins/), its crate in `plugin/` and `flick run plugin` to build it; `fullstack` is a server entry and a browser page (`target = "client"`) sharing the calls of a [`server fn`](../server-fn/) module, writes guarded by `Server.authenticated`, with Vite for the page |
| `init [--template app\|cli\|fullstack\|lib]` | the same files in the working directory; every file that exists is reported before anything is written (`FLICK-P019`) |
| `check [paths] [--watch] [--fix [--only codes]]` | checks; without paths the source and test roots; `--fix` applies the fixes that keep the program's meaning (such as removing a type annotation the value already has, `FLICK-T021`) to those files and reports what is left; `--only E002,F001` applies only the fixes of those codes, for a migration that changes nothing else |
| `build [paths] [--profile] [--labels all\|components\|none] [--emit js\|ts\|go] [--emit-dts] [--promise-facade] [--emit-ir[=json\|cbor]] [--schema-aot] [--out-dir] [--watch]` | writes ESM, source maps and `.iface` files (`--emit go`: a Go executable per entry, see [Go target](../codegen/#go-target)); `--emit-dts` adds `.d.ts` files and `--promise-facade` Promise façades `A/B.promise.js` (see [TypeScript](../codegen/#typescript)); `--emit-ir` adds `program.ir.json` (or `program.ir.cbor`): the IR the backend gets, optimized in a release build, with the version of the IR it is written in; shows build phases on stderr, then the module count and entry runners, or error counts and locations; `--silent`/`--quiet` and `--diagnostics json` suppress progress and the summary |
| `build [paths] --inputs` | prints a conservative JSON input inventory (`key`, `files`, `roots`, `modules`) without writing build artifacts or cache entries |
| `run [name] [--entry M] [--profile] [--devtools[=url]] -- args` | builds JavaScript (a Go executable when `emit = "go"`) and runs the `main` of the entry `name` (the only one when left out); a `name` that is not an entry runs a script of [`[scripts]`](../project/#scripts), its shell commands in order until one fails, with `args` as their `"$@"`; a client entry (`target = "client"`) builds with Vite and serves the page with Vite preview; `--devtools` sends server entry spans to the Effect DevTools ([observability](../observability/#devtools)) |
| `dev [name] [--entry M] [--profile] [--devtools[=url]] -- args` | rebuilds on changes and replaces the running app; in a project with several entries and none named, it runs every entry that is not a client's and, if one is, Vite (`node_modules/.bin/vite`) for the pages, which replaces a changed module in the page keeping the labelled atoms' values |
| `test [paths] [--filter] [--jobs] [--seed] [--reporter human\|json] [--watch] [--profile] [--emit js\|go]` | runs the tests of the test root, on Node or as a Go executable |
| `fmt [paths] [--check] [--stdin --stdin-filepath f]` | writes the canonical layout ([formatting](../grammar/#formatting)); without paths the source and test roots |
| `clean [--cache]` | removes the build artifacts; `--cache` also the build cache |
| `install [--locked]` | installs with pnpm (`--frozen-lockfile`, and `--offline` with the global `--offline`); at a workspace root, each member's |
| `add <pkg>… [--dev]`, `remove <pkg>…`, `update [pkg]…` | changes dependencies with pnpm |
| `deps [--why <pkg>]` | the installed graph, and the Flick modules of each direct dependency |
| `pack --out-dir <dir>` | a release build and its npm tarball; refuses `todo` and `echo` (`FLICK-P024`), and warns when the public API breaks importers since the latest tag |
| `diff [ref]` | the public API changes since a git revision (the latest tag by default), each `MAJOR` (removed or changed) or `MINOR` (added), and the version they call for from `package.json` |
| `ffi import <pkg> --out <dir> [--select a,b]` | reads the `.d.ts` of an installed package and writes `<dir>/<Pkg>/Generated.fl` with a coverage report; `--select` imports only the named exports (see [Importing a package's `.d.ts`](../ffi/#importing-a-packages-dts)) |
| `ffi check [paths]` | checks generated and reviewed bindings and their policies against the installed packages; without paths the source root |
| `schema export <M.codec> --target json-schema-2020-12 --out <file>` | the strict JSON Schema of a `PureCodec Json a` |
| `schema export <M.api> --target openapi-3.1 --out <file>` | an `HttpApi.Api` value as an OpenAPI 3.1 document, the same document `HttpApi.openapi` makes (see [Clients and documents](../http-api/#clients-and-documents)) |
| `doc --out-dir <dir>` | Markdown pages of the public API: types, kinds, constraints, effects, captures |
| `explain <code>` | the explanation of a diagnostic code, a plugin's too, as `std/L001` |
| `expand [paths]` | the code each [plugin](../plugins/) block of the project or of the files stands for |
| `doctor` | the compiler, Node.js, pnpm, the TypeScript Content Mapper, the installed runtime and Effect, the optional adapters |
| `inspect <M> --emit ast\|typed-ir\|js\|ts\|iface\|mapping` | one representation of a module |
| `lsp --stdio` | the [language server](../editor/#language-server); stdio is its only transport |
| `content-mapper [--declarations <paths>…]` | serves TypeScript's [Content Mapper](../editor/#content-mapper) requests for `.fl` files on stdio, started by `@flick-lang/content-mapper`; `--declarations` instead writes the projection of each `.fl` file under the paths beside it as `Name.d.fl.ts` and exits |

At the root of a workspace without `[project]`, `check`, `build`, `test`,
`fmt` and `clean` without paths, and `install`, run on every member,
dependencies first.

`check`, `build` and the build steps of `run`, `dev` and `test` install
nothing, run no `main` and load no foreign module. An installed
`@flick-lang/runtime` or `effect` of another version than the compiler
pins stops them with `FLICK-P015` (exit code 2). `schema export` loads the
module that makes the codec, whose top level is pure, and nothing else.

`test` streams human results after each test's cleanup, with success/failure markers, completion counts and timings, then prints a summary. Node and Go share the format and honor `--color`; auto color checks stdout, `NO_COLOR` and `TERM=dumb`. `--reporter json` writes one final object in declaration order, without color or timings.

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

`run` and `dev` build JavaScript, or a Go executable per entry when
`emit = "go"`. Each build
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

`install` and `update` first fetch the Flick libraries `flick.lock` pins,
and `add` with `--git`, `--url` or `--path` and `remove` change
`[dependencies]` (see [project.md](../project/#flick-dependencies)). For npm
packages the dependency commands delegate to pnpm. Every command that changes
`package.json`, its lockfile or the tree first checks that `pnpm --version`
is the version `packageManager` pins (`FLICK-P016`, exit code 2, nothing
changed). `install --locked` without `pnpm-lock.yaml`, or with one out of
date, is `FLICK-P017` with exit code 2.

`pack` builds the project in the release profile and refuses a `todo` or
`echo` left in it (`FLICK-P024`). A library with `[package]` packs its sources:
`<name>-<version>.tgz` holds its `flick.toml`, its source directory and its
readme and license under `package/`, without times or owners, so the same
sources give the same bytes; stdout shows the sha256 a `url` dependency
names. An app packs a staging package, with the runner of each entry, which the staged
`package.json` names in `bin` (see [project.md](../project/#entry-points)), checks that every module has its `.iface` (and
`.d.ts` with `emit-dts`), that interfaces in the package agree on each
other's fingerprints and that the paths `exports` names exist
(`FLICK-P020`), and writes the tarball with `pnpm pack`.

## Streams and exit codes

stdout carries only the result: the app's output, the test reporter, a
printed representation. Diagnostics and service messages go to stderr.
With `--diagnostics json` stderr is NDJSON, one event per line, every event
with `schema-version`: `diagnostic`, `log` (service messages),
`application-stderr` (the app's or the tests' stderr) and
`package-manager-output` (pnpm), the last three as
`{ "type", "channel", "text" }` without ANSI codes. A `diagnostic` has
`code` (`FLICK-…`), `severity` (`error`, `warning`, `note`), `message`, `path` and
`range` (`start`, `end`: UTF-8 byte offsets, end exclusive; both `null`
without a location), then `related` locations with their messages and
`fixes` (a message and edits, each a `path`, a `range` and a `replacement`).
In human mode the app's stderr passes through as it is.
`test --reporter json` writes its object to stdout, and the stdout of
`flick lsp` carries only the protocol.

`--silent` is an alias for `--quiet`, accepted before or after the command.
It suppresses service output, not warnings or errors, and leaves useful stdout
(such as `build --inputs` JSON or the app's output) intact. It conflicts with
`--verbose`; `--warnings-as-errors` still promotes warnings and changes the exit code.

Explicit `build` and `build --watch` show one updating line with the current
phase and elapsed time when stderr is a TTY, `TERM` is not `dumb`, and `CI` is
unset. Pipes, CI and dumb terminals get one line per phase, without animation.
Color follows `--color` and `NO_COLOR`; disabling color does not disable animation.
The live line is cleared before diagnostics and the final summary. Internal builds
in `run`, `dev` and `pack` do not show this progress. `build --inputs` prints only
its JSON inventory on success, without progress or a summary.

| Code | Meaning |
| - | - |
| `0` | success; for `fmt --check`, every file is formatted |
| `1` | errors in the code, failed tests, an unformatted file, a failed app or cleanup |
| `2` | a wrong invocation or configuration, a missing or wrong dependency or toolchain |
| `3` | an internal error of Flick |
| `130` / `143` | SIGINT / SIGTERM stopped the app and its cleanup succeeded |
