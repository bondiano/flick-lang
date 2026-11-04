---
title: "Project"
---

A project is a directory with `flick.toml` (see [Compiler CLI](../cli/)). Commands find the nearest
`flick.toml` from the working directory upwards; `--project <path>` selects a
project directory or a configuration file explicitly. Several projects form a
[workspace](#workspaces), and `--package <name>` selects one of its members.

## `flick.toml`

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
promise-facade = false
schema-aot = false

[sql]
dialect = "postgresql"

[lint]
allow = ["FLICK-T021"]
no-let-assert = false
max-function-lines = 60
max-nesting = 4
max-complexity = 10
max-params = 5
```

| Key | Values | Default |
| - | - | - |
| `schema-version` | `1`; required, checked before anything else | — |
| `project.kind` | `"app"` or `"lib"`; required | — |
| `project.source-dir` | directory of source modules | `"src"` |
| `project.test-dir` | directory of test modules | `"test"` |
| `project.entry` | module that exports `main`, its own or re-exported: the entry `main`, whose runner is `flick-main.js` | — |
| `project.entries` | several entries, `{ server = "App.Server", cli = "App.Cli" }`: a runner `flick-<name>.js` each; an entry with a runner of its own is a table, `web = { module = "App.Worker", runner = "Runner.Cloudflare" }`; `target = "client"` marks an entry that runs in a browser: nothing its `main` reaches may need `Server` (`FLICK-E009`, with the path to it), so its bundle keeps no server module; names are lower-case letters, digits and `-`; instead of `entry` | — |
| `project.runner` | the module that starts each entry without a runner of its own: it exports `type alias Main`, the type `main` must have (`FLICK-P026` otherwise), and `run`, which takes `main` and whose result the runner exports as `default`; built in are `"Runner.Node"`, `"Runner.Browser"` and `"Runner.Cloudflare"`, a Worker whose `fetch` gives `main : Runner.Cloudflare.Fetch ->{Runner.Cloudflare.Env; AppError} Http.ServerResponse.ServerResponse` the request and the context (`wait_until`), and provides the bindings as the service `Env` (`Runner.Cloudflare.var`, `binding`); a runner may provide services this way, and `main` may need any of those its `Main` names; a package may provide another; without one, a pure `main` is called and an effect runs as on Node | — |
| `build.out-dir` | base directory of artifacts; not the project's root, the directory of its sources or tests, or one above them (`FLICK-P028`, before anything is written) | `"dist"` |
| `build.profile` | `"debug"` or `"release"` (see [codegen.md](../codegen/)); `--profile` of `build` and `test` overrides it | `"debug"` |
| `build.emit` | `"js"`, `"ts"`, or `"go"`: a Go module per entry under `go/<entry>/`, which `go build` compiles into the executable `flick-<entry>`; Go must be on `PATH` (`FLICK-P029`), an import of a module of the js target only (`Ui`, `Atom`, `Action`, `Rpc`, `Otlp`, `DevTools`, `Js`, the `Node`/`Browser`/`Cloudflare` runners) or of a package not built for go is `FLICK-P030` already in `flick check`, and what the go target does not compile yet is `FLICK-P012` (see [Go target](../codegen/#go-target)) | `"js"` |
| `build.emit-dts` | boolean | `false` |
| `build.promise-facade` | boolean | `false` |
| `build.schema-aot` | boolean | `false` |
| `build.solver-steps` | instance resolution steps per definition | `100000` |
| `build.solver-depth` | depth of nested class constraints | `256` |
| `build.labels` | what a release build labels `Module.name` for debugging tools: `"all"` (atoms, views, actions), `"components"` (views and actions) or `"none"`; debug labels all; `flick build --labels` overrides it | `"all"` |
| `build.effect` | what runs effects: `"effect"`, or `"none"` for plain JavaScript that imports only `@flick-lang/runtime/core`: then `main` is `Unit -> Unit`, a runtime call is `FLICK-E001`, and a `task`, a refined type, a schema or a built-in module outside `Prelude`, `List`, `Option`, `Result`, `String`, `Float` and `Bool` is `FLICK-E008`. A library on Effect also writes, under `_core/` of its output, the modules that need no Effect; a library a project depends on is compiled with the project's `effect` | `"effect"` |
| `build.ui` | the module that renders and mounts views in place of React: it exports every name `Ui` and `Action` bind from `@flick-lang/runtime/ui` and `@flick-lang/runtime/ui/react`; a relative path is relative to the project's root | React |
| `sql.dialect` | the SQL dialect of `sql table` and `sql { … }` (see [sql.md](../sql/)): `"postgresql"`; `"mysql"`, `"sqlite"`, `"clickhouse"` and `"oracle"` are recognized but not supported yet | `"postgresql"` |
| `lint.allow` | warning codes not reported, such as `"FLICK-T021"`; errors cannot be turned off | `[]` |
| `lint.no-let-assert` | `true` makes every `let assert` an error (`FLICK-T027`), in `flick check`, `build`, `test` and the editor | `false` |
| `lint.max-function-lines` | lines of a function body past which `FLICK-T036` warns | `60` |
| `lint.max-nesting` | how deep `if`, `match`, `fun`, `do`, `task` and `for` nest before `FLICK-T037` warns | `4` |
| `lint.max-complexity` | branches of a function (cyclomatic complexity) past which `FLICK-T038` warns | `10` |
| `lint.max-params` | parameters of a function past which `FLICK-T039` warns | `5` |

Errors in the configuration exit with code `2`:

| Problem | Diagnostic |
| - | - |
| TOML syntax, an unknown key, a value of the wrong type or outside its set | `FLICK-P005` |
| a `schema-version` other than `1`, or none | `FLICK-P006` |
| `kind = "app"` without `entry` or `entries`, or with an empty `entries` | `FLICK-P007` |
| both `entry` and `entries`, or an entry name that is not one | `FLICK-P005` |
| no project found, or `--project` names none | `FLICK-P008` |
| a command needs a project at a workspace root, `--package` names no member, a member has no `flick.toml`, a path dependency is not a library, or dependencies form a cycle | `FLICK-P031` |
| an entry is not a source module that exports `main` | `FLICK-P011` |
| an entry `main` does not take and return `Unit`, needs a service or can fail with an error other than `AppError` | `FLICK-P026` |
| `sql.dialect` names a dialect that is not supported yet | `FLICK-P022` |
| `sql.dialect` names no dialect; the message lists the known ones | `FLICK-P023` |
| `lint.allow` names an unknown code | `FLICK-P005` |

## Entry points

An app's entries are the programs it builds: `flick build` writes a runner
`flick-<name>.js` next to the modules for each, and the modules once, so two
entries share what they import. A runner imports the entry's `main`, from the
module that defines it when the entry re-exports it, and starts it; it begins
with `#!/usr/bin/env node`, so `flick pack` names each runner in the
package's `bin`, and the only entry `main` under the package's name.
`flick run <name>` and `flick dev <name>` start one; with a single entry the
name may be left out.

Every entry exports `main : Unit -> Unit` or a runtime function taking and
returning `Unit`, requiring no services and failing only with `AppError`
(an empty error row is valid too). Provide required layers and handle other
errors inside `main`, not in the launcher.

A pure `main` is called directly. Normally its launcher imports no runtime;
`schema-aot = true` still imports the schema installation helpers.
A runtime `main` is started through the entry runner, which observes its
final `Exit` (see [code generation](../codegen/)). Invalid signatures produce
`FLICK-P026` during project checking, or during a standalone file build.

```toml
[project]
kind = "app"
entries = { server = "App.Server", migrate = "App.Migrate" }
```

Entries of a web page are something else: a bundler such as Vite takes
JavaScript entry modules, which import the `.fl` modules they need (see
[codegen.md](../codegen/)); `entries` are the programs the runtime starts.

## Workspaces

A `flick.toml` with `[workspace]` groups projects, its members, by their
directories:

```toml
schema-version = 1

[workspace]
members = ["contract", "server"]
```

A root without `[project]` is no project itself, and takes only
`[workspace]` and `[scripts]`. There, `flick check`, `build`, `test`, `fmt`
and `clean` without paths, and `flick install`, go through every member, each
after the members it depends on by path, and fail if one does; `flick run`
runs a script, and `dev` and the other commands need a member, `--package <name>` (`-p`), the name of its directory.
In a member, commands run on that member as usual, and `--package` picks
another.

## Flick dependencies

A project compiles the Flick libraries it depends on from their sources.
`[dependencies]` names each with one source: a directory relative to the
`flick.toml`, a tag of a git repository (with the library's directory in it,
`subdir`), or a gzipped tarball with its sha256:

```toml
[dependencies]
contract = { path = "../contract" }
std = { git = "https://github.com/bondiano/flick-lang", tag = "std-v0.1.0", subdir = "packages/std" }
text = { url = "https://example.com/text-1.0.0.tgz", sha256 = "<64 hexadecimal digits>" }
```

The library's directory holds a `flick.toml` of `kind = "lib"`; its
`source-dir` is where the modules are, and its own dependencies come along.
A library others fetch says what it is and which compilers build it, the
last a semver requirement:

```toml
[package]
name = "text"
version = "1.0.0"
flick = ">=0.1, <0.2"
```

A library that provides a plugin names its WebAssembly component, a path
inside the library (see [plugins.md](../plugins/)):

```toml
[package]
name = "pairs"
version = "1.0.0"
plugin = "pairs.wasm"
```

`flick install` fetches each git or url library the manifests name, directly
or through a library, into the package store (`FLICK_STORE`, else
`flick/pkg` in the user's cache directory), where each sits under the hash of
its files, and pins it in `flick.lock`: the commit of a git tag and that
hash. Commit `flick.lock`: a build reads the store through it and fetches
nothing. `flick install --offline` uses only the store, `--locked` fails
rather than change the lock, `flick update <name>` fetches a library again,
`flick add <name> --git URL --tag T [--subdir S]` (or `--url U --sha256 H`,
or `--path DIR`) adds one and `flick remove <name>` drops it.

A library is compiled like a [source package](#packages): for the target the
project emits, without its warnings, after the project's own modules, as
another package, so instances of its classes and types are its own. A change
to a path library's sources or its `flick.toml`, or to `flick.lock`, rebuilds
the project, and `--watch` follows them. npm dependencies stay in
`package.json`.

## Scripts

`[scripts]` names commands to run in order, each a shell command (`sh -c`,
`cmd /C` on Windows) in the directory of the `flick.toml`, with the `flick`
that runs them first on `PATH`. A step `@name` runs another script of the same
file:

```toml
[scripts]
lint = ["flick fmt --check", "flick check --warnings-as-errors"]
test = "flick test"
ci = ["@lint", "@test"]
```

`flick run ci` runs one, looking in the project's `flick.toml`, then in
the workspace root's; an entry of the project with the same name runs
instead. The steps stop at the first
that fails, with its exit code. Arguments after `--` are each step's `"$@"`.

## `flick check`

Without paths `flick check` checks every module of the source and test roots.
With paths it checks those files and directories, together with every module
they import, transitively. Outside a project paths are required.

## Packages

npm packages live in `package.json`; pnpm installs them. Flick reads the
installed tree: for each dependency of `package.json` it reads
`node_modules/<name>/package.json`. An npm package with Flick modules is a
source package, which names the directory of its sources:

```json
{ "name": "@acme/contract", "flick": { "source-dir": "src" } }
```

Each dependent compiles its modules for the target it emits, js or go, as
modules of that package. So a JS client and a Go server that both depend on it
share its types, and its constructors have the same tags on both sides. A
source package needs no build of its own; within a repository, a
[path dependency](#flick-dependencies) names it without `node_modules`. Its modules are checked
for the target of the dependent, so on go an import of a js-only module is
`FLICK-P030`. The dependent does not report the package's warnings. Its own
modules come first: a module the project declares is not looked up in the
package.

| Problem | Diagnostic |
| - | - |
| an imported module is found nowhere while a dependency is not installed | `FLICK-P009`: run `flick install` (exit code `2`) |
| an imported module is found nowhere | `FLICK-R003`: add the package with `flick add <package>` |
| a dependency names no source, or two, or `[package]` is not semver | `FLICK-P032` |
| a library's `flick` requirement leaves this compiler out | `FLICK-P033` |
| a git or url library is not in `flick.lock` or in the store | `FLICK-P034`: run `flick install` |
| a plugin a module imports cannot be found or loaded | `FLICK-P035` |
| a plugin gave no code for a block, or failed | `FLICK-P036` |
