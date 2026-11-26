---
title: "Command line"
---

`flick` is one binary over the compiler core. Every command takes the
global options `--project <path>`, `--package <name>` (`-p`, a member of the
[workspace](../project/#workspaces)), `--diagnostics human|json`,
`--warnings-as-errors`, `--color auto|always|never`, `--quiet` (alias `--silent`)/`--verbose`,
`--timings` ([what took the time](#timings)),
`--offline` (no network: [plugins](../plugins/#reaching-outside) answer
from the project's snapshots, which `CI` set means too, and `install`
installs from the store), `--allow-tool <tool>` (lets the selected backend
run a host tool in a build outside a project; a project allows
its tools in [`[tools]`](../project/#flicktoml), and the flag there is
`FLICK-P051`) and `--help`; `flick help <command>` prints a command's help and
`flick --version` the compiler's version (`flick doctor` shows the rest).

Built-in plugins, the project's backend and the plugins of its dependencies may
add commands of their own. `flick --help` lists them by contributor, and they
run as `flick <command>` or `flick <contributor>:<command>` (see
[Plugin-contributed commands and backends](#plugin-contributed-commands-and-backends)
and [Contributed commands](../commands/)).

## Commands

| Command | What it does |
| - | - |
| `new <dir> [--target <name>] [--template <name>] [--from <source> [--tag <tag> [--subdir <dir>] \| --sha256 <hex>]]` | creates a project from a template contributed by the selected target backend (see [Project templates](#project-templates-new-and-init)) or a third-party `--from` package (local path, tarball URL with `--sha256`, or git repository with `--tag` and optional `--subdir`); every template file, including `flick.toml`, comes from the contributor, `--from` pins the template under `[[template]]` in `flick.lock`, an unsupported template is `FLICK-P060` listing the available ones, and an unknown `--target` is `FLICK-P059` |
| `init [--target <name>] [--template <name>] [--from <source> [--tag <tag> [--subdir <dir>] \| --sha256 <hex>]]` | adds the template files to the working directory while keeping existing unrelated files; every colliding template file is reported before anything is written (`FLICK-P019`), except an existing `AGENTS.md`, which is kept, and `flick.lock`, into which new pins are merged |
| `check [paths] [--watch] [--fix [--only codes]]` | checks; without paths the source and test roots; `--fix` applies the fixes that keep the program's meaning (such as removing a type annotation the value already has, `FLICK-T021`) to those files and reports what is left; `--only E002,F001` applies only the fixes of those codes, for a migration that changes nothing else |
| `prove [paths] [--watch]` | verifies the proof modules of the proof root (`project.proof-dir`) and the function contracts (`where require ... ensure ...`) of the project's source and test modules, or of the files given, with the code they name; `check`, `build` and `test` type-check contracts without proving or lowering them and never read the proof root (see [the proof contract](https://github.com/bondiano/flick-lang/blob/main/docs/formal/proofs.md)) |
| `build [paths] [--target <name>] [--profile] [--labels all\|components\|none] [--emit-ir[=json\|cbor]] [--out-dir] [--watch] [--no-native-overrides] [<backend options>]` | compiles the project with the selected target backend; target-specific flags are declared by the selected backend (see [Target and backend options](#target-and-backend-options)); `--emit-ir` adds `program.ir.json` (or `program.ir.cbor`): the IR the backend gets, optimized in a release build, with the version of the IR it is written in; `--no-native-overrides` lowers Flick fallback bodies instead of `@native` overrides for conformance testing; shows build phases on stderr, then the module count and entry runners, or error counts and locations; `--silent`/`--quiet` and `--diagnostics json` suppress progress and the summary |
| `build [paths] --inputs` | prints a conservative JSON input inventory (`key`, `files`, `roots`, `modules`) without writing build artifacts or cache entries |
| `run [name] [--target <name>] [--vm-tier <tier>] [--entry M] [--profile] [--no-native-overrides] [<backend options>] -- args` | builds with the selected target backend and runs the `main` of the entry `name` (the only one when left out), or with `--target vm` runs a project entry or standalone `.fl` file in process on the VM; a `name` that is not an entry runs a script of [`[scripts]`](../project/#scripts), its shell commands in order until one fails, with `args` as their `"$@"`; a client entry (`target = "client"`) delegates to the selected backend's client runner (`FLICK-P067` if the backend has none) |
| `dev [name] [--target <name>] [--entry M] [--profile] [--no-native-overrides] [<backend options>] -- args` | rebuilds on changes and replaces the running app; in a project with several entries and none named, it runs every non-client entry and, if any entry has `target = "client"`, delegates the client runner to the selected backend |
| `test [paths] [--target <name>] [--vm-tier <tier>] [--filter] [--list] [--forbid-only] [--jobs] [--seed] [--timeout] [--reporter human\|json] [--watch] [--profile] [--no-native-overrides] [<backend options>]` | runs the tests of the test root with the selected target backend (see [testing](../testing/)) |
| `fmt [paths] [--check] [--stdin --stdin-filepath f]` | writes the canonical layout ([formatting](../syntax/#formatting)); without paths the source and test roots |
| `clean [--cache]` | removes the build artifacts; `--cache` also the build cache |
| `install [--locked]` | installs Flick `[dependencies]` from `flick.lock` and delegates target package installation to the selected backend; at a workspace root, each member's |
| `add <pkg>… [--dev]`, `remove <pkg>…`, `update [pkg]…` | manages Flick `[dependencies]` (`--path`, `--git`, `--url`) and delegates target-ecosystem package operations to the selected backend (`FLICK-P063` if the backend manages no packages of its own) |
| `deps [--why <pkg>]` | shows the Flick dependency graph and delegates target package inspection to the selected backend |
| `plugins` | the plugins of the project's path, git and url dependencies, the compiler's own first: where each comes from, the hash of its component and whether `flick.lock` pins that build |
| `pack --out-dir <dir> [--target <name>]` | writes a deterministic Flick source tarball (printing `<sha256>  <path>` on stdout, installable via `flick add --url --sha256`) and any optional artifact from the selected backend's `pack` command; refuses `todo` and `echo` (`FLICK-P024`), and warns when the public API breaks importers since the latest tag |
| `diff [ref]` | the public API changes since a git revision (the latest tag by default), each `MAJOR` (removed or changed) or `MINOR` (added), and the version they call for |
| `doc --out-dir <dir> [--source-base-url <url>] [--check-examples] [--target <name>] [--std]` | Markdown pages of the public API: types, kinds, variances, constraints, effects, captures; with `--check-examples`, compiles and executes examples on the selected backend (or `--target <name>`) |
| `explain <code>` | the explanation of a diagnostic code, a plugin's too, as `std/L001` |
| `expand [paths]` | the code each [plugin](../plugins/) block of the project or of the files stands for |
| `doctor [--target <name>]` | checks the compiler, package store, git, plugins, and the selected target backend's environment and package checks |
| `inspect <M> --emit ast\|typed-ir\|output\|iface\|variance\|mapping [--target <name>] [--no-native-overrides] [<backend options>]` | one representation of a module; `--emit output` asks the selected backend for the module's emitted code; `--emit iface` inspects the typed interface without loading any backend; `--emit variance` prints inferred parameter variances of exported types and classes (including installed packages and built-in modules, reporting unknown metadata as `inv (unknown)`) |
| `lsp --stdio` | the [language server](../editor/#language-server); stdio is its only transport |
| `repl [--target <name>] [--vm-tier <tier>]` | starts an interactive Flick REPL session on the in-process VM (`--target vm`, default); evaluates declarations and expressions with persistent bindings, `Host.live` services, auto-awaited `Eff` tasks, `SIGINT` interruption with scope finalization, and session commands (`:handle`, `:provide`, `:bg`, `:jobs`, `:kill`, `:quit`) |

At the root of a workspace without `[project]`, `check`, `build`, `test`,
`fmt` and `clean` without paths, and `install`, run on every member,
dependencies first.

`check`, `build` and the build steps of `run`, `dev` and `test` install
nothing, run no `main` and load no foreign module. An installed runtime
dependency of another version than the selected backend pins stops them with
`FLICK-P015` (exit code 2).

`test` streams human results after each test's cleanup, with success/failure markers, completion counts and timings, then prints a summary. Backends share the format and honor `--color`; auto color checks stdout, `NO_COLOR` and `TERM=dumb`. `--reporter json` writes one final object in declaration order, without color or timings.

## Target and backend options

`build`, `run`, `dev`, `test`, `pack`, `doctor`, `inspect` and `doc` (when
checking examples) take `--target <name>` to select which backend component
runs (an official backend short name, the short name of a pinned
`[build.backend]`, or, in a project and with a compiler built from the
repository, `lua` for its own `backends/lua` component). Without `--target`, the target comes from
`[build.backend]`, then `[build] target`, then the backend prefix of
`[build] environment`, and otherwise the default target backend (see
[Plugin-contributed commands and backends](#plugin-contributed-commands-and-backends)).
Outside a project, `flick doctor` without `--target` runs only the core checks
(compiler, store, git, plugins) and notes that no backend was selected; pass
`--target <name>` to include a backend's checks. An unknown target is
`FLICK-P059` (exit code `2`).

Backend-specific flags are declared by the selected backend component itself,
per command. `flick <cmd> --help` and `flick help <cmd>` display only the
selected backend's options for that command under `Options of \`<id>\`:` (pass
`--target <name> --help` to inspect another target's options). Options are also
configured persistently in `flick.toml` under `[target.<name>]`; CLI flags
override `flick.toml` (boolean flags accept `--<flag>=false` to override `true`
in `flick.toml`).

Passing an unknown option, a wrong-verb option, a cross-backend option or an
invalid value fails with `FLICK-P058` (exit code `2`). Legacy hardcoded target
flags (`--emit <target>`, `--emit-dts`, `--rt`) are removed with no
compatibility aliases and fail with `FLICK-P057` (exit code `2`), naming their
`--target` and `[target.<name>]` replacement (see the backend sections under
[Plugin-contributed commands and backends](#plugin-contributed-commands-and-backends)).

Arguments after `--` (such as `flick run app -- --rt`) are passed straight to
the program or script and are never parsed by `flick`.

### Project templates (`new` and `init`)

`flick new <dir>` and `flick init` take their project templates from the
selected target backend (`--target <name>`, or the default target backend when
omitted) or from a third-party package named by `--from` (which conflicts with
`--target`). When `--template <name>` is omitted, `flick` picks `app` if the
contributor declares one, or the sole template if it declares only one.
Requesting an unsupported template fails with `FLICK-P060` (exit code `2`) and
lists every available template with its summary.

Third-party template packages (`--from <source>`) accept a local directory
(`--from ./my-templates`), a gzipped tarball URL with its digest
(`--from https://example.com/tpl.tgz --sha256 <64-hex>`), or a git tag
(`--from https://example.com/repo --tag v1.0.0 [--subdir pkg]`), using the same
verification and package store as `flick add`. Remote `--from` URLs without
`--sha256` or `--tag` are refused (`FLICK-P032`). The generated project records
the verified source and component digest under `[[template]]` in `flick.lock`
(preserved by `flick install`). Official release-asset backends are pinned
under `[[backend]]` in `flick.lock` so later builds work offline. For the
templates bundled by official backends, see
[Plugin-contributed commands and backends](#plugin-contributed-commands-and-backends).

Template plans are pure file plans: any template returning host tool steps,
`resume`, `needs`, or escaping paths is refused with `FLICK-P055` before
anything is created. `flick init` keeps unrelated files and any existing
`AGENTS.md`, merges new lockfile pins into an existing `flick.lock`, and
refuses any other colliding file with `FLICK-P019` before writing.

## Outputs and the cache

A build writes into `build.out-dir` (or `--out-dir`) and records the files
it wrote in `.flick/artifacts.json`. After successful publication, obsolete
manifest-owned files are removed, including after an emit-mode change or a
cache hit. `clean` removes the remaining owned files and empty directories;
other files in a shared output directory stay. Files built alone, outside a
project, list what the build wrote in `.flick-files` of the output instead,
and the next build into it removes the listed files it no longer writes.

The cache in `.flick/cache/` keeps the artifacts of each build by a key over
the normalized selected roots separately from the available sources of the
project, its effective configuration, the profile, target options
and output settings, the compiler (version and binary), the backend component
and its pinned runtime graphs, and the package inputs reported by the selected
backend (its manifests, lockfiles, and the Flick sources of installed
packages). Any change of these is a miss; a hit writes exactly the files the
build would.

Watch commands compare individual paths and their state, including file
creation and deletion; one unrelated timestamp cannot hide another edit.

## Running and replacing an app

`run` and `dev` build with the selected target backend. Each build
goes into an atomically reserved generation `.flick/run/<pid>-<n>/`, never
written again once its process starts, so a running program always sees the
files it started with. The CLI owns the generation from construction through
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
`[dependencies]` (see [project.md](../project/#flick-dependencies)). The
target-ecosystem portion of `install`, `add`, `remove`, `update` and `deps` is
delegated to the project's selected backend, along with the closure's
`[target.<name>.dependencies]` for that backend. When the selected backend
manages no packages of its own, `flick install` handles Flick `[dependencies]`
alone without invoking a target package manager, while target package
operations such as `flick add <pkg>` (without `--path`, `--git` or `--url`)
fail with `FLICK-P063` (exit code 2) naming the backend and suggesting those
source options. Target-specific package managers and version checks are
documented under
[Plugin-contributed commands and backends](#plugin-contributed-commands-and-backends).

`pack` checks or release-builds the project for the selected target, refuses any
`todo` or `echo` left in it (`FLICK-P024`), and warns when the public API breaks
importers since the latest git tag. Whenever `flick.toml` declares `[package]`
(`name` and `version`), core writes a deterministic Flick source tarball
`<name>-<version>.tgz` containing `flick.toml`, the source directory (excluding
test companions), any declared `[package] plugin` component, and readme/license
files under `package/`, without timestamps or owners so the same sources give
the same bytes; stdout prints `<sha256>  <path>` for `flick add --url --sha256`.
A library inheriting `[[workspace.plugins]]` is refused (`FLICK-P032`). If the
selected backend also declares an optional `pack` command, core verifies emitted
`.iface` files (`FLICK-P020`) and delegates target packaging to that backend
(if a backend artifact collides with the Flick source tarball filename, it is
placed under `<out-dir>/<target>/`). Backends without a `pack` command run a
target-aware frontend check and write only the Flick source tarball.

## Checked package documentation

`flick doc --out-dir api --check-examples` writes Markdown from the checked
public interface and its `///` comments, and checks their fenced Flick examples
with the package imports available. Runnable examples compile in memory and
execute through the project's selected backend (or `--target <name>`), using
that target's own toolchain and runtime. Parse, type, undeclared-import
(`FLICK-P062`), unsatisfied `fail` (`FLICK-P062`), and runtime (`FLICK-P061`)
failures report diagnostics with source spans inside the original `///` comment
and write no files. Private declarations are not separate API entries. A public
re-export uses the original declaration's documentation.

`--source-base-url https://host/repo/blob/REV/package` links declarations to
paths relative to the project root and their original source lines. Choose a
fixed revision for reproducible links. Without this option, no remote URL is
guessed. `--std` documents the compiler's checked built-in modules instead of
compiling the standard library as a user project. With `--std`, the source URL maps the repository root (`packages/core` and `packages/std`).

The site's [standard library API](../../api/) consumes this Markdown directly;
there is no separate site-specific signature or doc JSON model. API declarations
and source comments are shared between English and Russian navigation. See
[Your own typed DSL](../../handbook/typed-dsl/) for generated declarations and
the boundary between compilation errors and domain errors.

## Streams and exit codes

stdout carries only the result: the app's output, the test reporter, a
printed representation. Diagnostics and service messages go to stderr.
With `--diagnostics json` stderr is NDJSON, one event per line, every event
with `schema-version`: `diagnostic`, `log` (service messages),
`application-stderr` (the app's or the tests' stderr) and
`package-manager-output` (target tool output), the last three as
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

### Timings

`--timings` reports on stderr, after the diagnostics, how long the command's
phases took, the work measured inside them (such as checking the built-in
modules), the ten modules that took longest to check and, when the build
cache missed, the inputs that differ from the build cached last. What a
command spends in a process it starts, the app or the tests, is the phase
`Running a child process`. A watching command reports each run: `check` and
`build` after it, `test --watch` and `dev` after its build. With `--diagnostics json` the report is one event of type
`timings` with `stages` and `phases` (each a `label` and `micros`), `modules`
(each a `module` and `micros`, the slowest first) and `cache-misses`. The
measurements are no input of the build cache.

The report also says what the module cache did with each module of the
run: `reused` (kept in memory by a watching command), `taken from the store`
(saved by an earlier process), `not storable` (a module a saved key does not
cover, such as one with inline modules), `no earlier entry`, or the inputs
that differ from those of the entry kept or saved last: `source`, `file`,
`config`, `mode`, `compiler`, `project modules` (a module added to or
removed from the run), `plugin inputs`, `plugin output`, `module arguments`
and `import M`, followed by what changed in `M` (`import App.Base ← source`).
The human report counts them and lists the modules checked again, at most
twenty; with `--diagnostics json` the event has `module-cache`, each entry a
`module` and its `reason`, ordered by module. A reason names an input, never
its value. `cache-misses` explains why a whole build was not taken from the
build cache; these reasons explain the checking of each module.

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

## Plugin-contributed commands and backends

Everything above describes the backend-neutral core of `flick`. Built-in
plugins and official target backends contribute their own commands, target
options, project templates, dependency managers, packaging steps, and
environment checks (see [Contributed commands](../commands/)).

### Built-in `std`

| Command | What it does |
| - | - |
| `schema export <M.codec> --target json-schema-2020-12 --out <file>` | a command of `std` (`std:schema export`): exports the strict JSON Schema of a `PureCodec Json a` by running through the project's selected backend |
| `schema export <M.api> --target openapi-3.1 --out <file>` | a command of `std` (`std:schema export`): exports an `HttpApi.Api` value as an OpenAPI 3.1 document, the same document `HttpApi.openapi` makes, running through the project's selected backend (see [Clients and documents](../http-api/#clients-and-documents)) |

`schema export` (also invocable as `flick std:schema export`) compiles an
in-memory exporter module against the project and runs it through the project's
selected backend in an isolated temporary generation, writing no generated
modules to the project tree. A schema that cannot be exported strictly fails
with `FLICK-D008` pointing at the exported declaration.

### `flick/js` backend (`--target js`, default)

Selected by `--target js`, `[build] target = "js"`, or as the default target
backend when no target is configured.

| Command | What it does |
| - | - |
| `ffi import <pkg> --out <dir> [--select a,b]` | a command of the JavaScript backend (`js:ffi import`): reads the `.d.ts` of an installed package and writes `<dir>/<Pkg>/Generated.fl` with a coverage report; `--select` imports only the named exports (see [Importing a package's `.d.ts`](../ffi/#importing-a-packages-dts)) |
| `ffi check [paths]` | a command of the JavaScript backend (`js:ffi check`): checks generated and reviewed bindings and their policies against the installed packages; without paths the source root |
| `content-mapper [--declarations <paths>…]` | a session command of the JavaScript backend (`js:content-mapper`): serves TypeScript's [Content Mapper](../editor/#content-mapper) requests for `.fl` files on stdio, started by `@flick-lang/content-mapper`; `--declarations` instead writes the projection of each `.fl` file under the paths beside it as `Name.d.fl.ts` and exits |

- **Options (`[target.js]`)**: `--typescript` (`build`, `inspect`),
  `--declarations` (`build`), `--promise-facade` (`build`), `--schema-aot`
  (`build`; runs a `node` toolchain step, requiring `"node"` in `[tools]` or
  `--allow-tool node` outside a project), `--runtime rt|effect` (`build`,
  `run`, `dev`, `test`, `inspect`; default `rt`), `--devtools[=url]` (`run`,
  `dev`; sends server entry spans to the Effect DevTools, default URL
  `ws://localhost:34437`, see [observability](../observability/#devtools)), and
  `--ui <specifier>` (`build`, `run`, `dev`, `test`).
- **Project templates**: `app`, `cli` (a [`cli` block](../cli-dsl/) entry),
  `fullstack` (server and client entries sharing a [`server fn`](../server-fn/)
  module with Vite), `lib`, and `plugin` (a library with a [plugin](../plugins/)
  crate in `plugin/`).
- **Client runner**: client entries (`target = "client"`) run through Vite via
  the backend's `run-client` / `dev-client` commands (requiring `"node"` in
  `[tools]`).
- **Build cache inputs**: `package.json`, `pnpm-lock.yaml`, and the manifests
  and Flick sources of installed packages.
- **Dependencies**: dependency commands run `pnpm` (requiring
  `[tools] "flick/js" = ["pnpm"]` in `flick.toml`, else `FLICK-P051`). Every
  command that changes `package.json`, its lockfile or the tree first checks
  that `pnpm --version` is the version `packageManager` pins (`FLICK-P016`,
  exit code 2, nothing changed); the message names the pinned version and how
  to activate it, and `flick doctor` prints both versions. `install --locked`
  without `pnpm-lock.yaml`, or with one out of date, is `FLICK-P017` with exit
  code 2.
- **Packaging (`pack`)**: stages `package.json` with entry `bin` fields, checks
  JS `.iface`/`.d.ts` companions and `exports`, and optionally runs pinned
  `pnpm pack` to produce an npm tarball for apps with `package.json`.
- **Environment checks (`doctor`)**: reports Node.js, pnpm,
  `@flick-lang/runtime`, `effect`, the Content Mapper, and optional adapters
  without checking Go.

Removed legacy flags (`FLICK-P057`) for `flick/js`:

| Removed flag | Replacement |
| - | - |
| `build --emit js` / `test --emit js` | `--target js` (or omit; `js` is default) |
| `build --emit ts` | `--target js --typescript` (or `[target.js] typescript = true`) |
| `build --emit-dts` | `--declarations` (or `[target.js] declarations = true`) |
| `build\|run\|dev\|test --rt` | `--runtime rt` (default on `flick/js`; use `--runtime effect` for Effect) |
| `inspect <M> --emit js` | `inspect <M> --emit output --target js` |
| `inspect <M> --emit ts` | `inspect <M> --emit output --target js --typescript` |

### `flick/go` backend (`--target go`)

Selected by `--target go` or `[build] target = "go"`.

- **Options (`[target.go]`)**: `--cgo` (`build`, `run`, `dev`, `test`; enables
  `CGO_ENABLED=1`, default `false`).
- **Project templates**: `app`, `cli`, `lib`, and `plugin` (with
  `[build] target = "go"` and `[tools] "flick/go" = ["go"]`, and no
  `package.json`).
- **Dependencies**: manages no foreign packages of its own; `flick install`
  handles Flick `[dependencies]` without needing Node or pnpm, while target
  package operations such as `flick add <pkg>` (without `--path`, `--git` or
  `--url`) fail with `FLICK-P063`.
- **Client runner**: client entries (`target = "client"`) are unsupported
  (`FLICK-P067`).
- **Packaging (`pack`)**: declares no backend `pack` command; runs a
  target-aware frontend check and writes only the Flick source tarball without
  Node, pnpm, or `go`.
- **Checked documentation examples (`doc --check-examples`)**: compiles and
  executes runnable examples as native Go binaries without Node.
- **Environment checks (`doctor`)**: checks `go` and `[tools]` without checking
  Node.js or pnpm.

Removed legacy flags (`FLICK-P057`) for `flick/go`:

| Removed flag | Replacement |
| - | - |
| `build --emit go` / `test --emit go` | `--target go` (or `[build] target = "go"`) |

### Built-in `flick/vm` target (`--target vm`)

Selected by `--target vm`, `[build] target = "vm"`, or `[build] environment = "vm/native"`.
Runs compiled Flick programs and test suites directly in process on the built-in virtual machine without spawning `node` or `go`.

- **Options (`[target.vm]`)**: `--vm-tier <tier>` (`run`, `test`, `repl`; accepts `0`, `tier0`, `1`, or `tier1`, default `0`; also configurable via `[target.vm] tier = "0" | "tier0" | "1" | "tier1"` in `flick.toml`).
- **Running (`run`)**: executes a project entry (`flick run --target vm`) or a standalone source file (`flick run --target vm path/to/file.fl`) in process on the tier-0 reference VM (default) or the tier-1 register bytecode VM (`--vm-tier 1|tier1`), supporting `--profile debug` and `--profile release`. On tier 1, any reachable operation outside its pure synchronous subset fails with `FLICK-P012` naming the operation and its source span (no silent fallback to tier 0).
- **Testing (`test`)**: executes project test suites in process (`flick test --target vm`), supporting `--filter`, `--list`, `--seed`, `--reporter human|json`, and `--profile`.
- **Interactive REPL (`repl`)**: runs persistent REPL sessions in process (`flick repl`), supporting `Host.live` ambient services, auto-awaiting `Eff` tasks, `SIGINT` interruption with LIFO scope finalization, and session commands `:handle <expr>`, `:provide <expr>`, `:bg <expr>`, `:jobs`, `:kill <n>`, and `:quit`/`:q`.
- **Current scope**: `flick build --target vm` (standalone VM images, tracked in `#988`), `flick dev --target vm`, and `flick inspect --emit output --target vm` are not supported yet and exit with `FLICK-P012`.
- **Environment checks (`doctor`)**: reports the built-in VM availability and active tier without checking Node.js or Go.
- **Cargo feature**: enabled by default in `flick` (`vm` feature); builds compiled with `--no-default-features` exclude the VM and report `FLICK-P013` when `--target vm` is selected.
