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
target = "js"

[target.js]
typescript = false
declarations = false
promise-facade = false
schema-aot = false
runtime = "rt"

[sql]
dialect = "postgresql"

[lint]
allow = ["FLICK-T021"]
no-let-assert = false
unused-proof-premises = false
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
| `project.proof-dir` | directory of proof modules, which only `flick prove` checks | `"proofs"` |
| `project.entry` | module that exports `main`, its own or re-exported: the entry `main`, whose runner is `flick-main.js` | — |
| `project.entries` | several entries, `{ server = "App.Server", cli = "App.Cli" }`: a runner `flick-<name>.js` each; an entry with a runner of its own is a table, `web = { module = "App.Worker", runner = "Runner.Cloudflare" }`; `target = "client"` marks an entry that runs in a browser: nothing its `main` reaches may need `Server` (`FLICK-E009`, with the path to it), so its bundle keeps no server module; names are lower-case letters, digits and `-`; instead of `entry` | — |
| `project.runner` | the module that starts each entry without a runner of its own: it exports `type alias Main`, the type `main` must have (`FLICK-P026` otherwise), and `run`, which takes `main` and whose result the runner exports as `default`; built in are `"Runner.Node"`, `"Runner.Browser"` and `"Runner.Cloudflare"`, a Worker whose `fetch` gives `main : Runner.Cloudflare.Fetch ->{Runner.Cloudflare.Env; AppError} Http.ServerResponse.ServerResponse` the request and the context (`wait_until`), and provides the bindings as the service `Env` (`Runner.Cloudflare.var`, `binding`) and the live browser services `Host.BrowserLive`; a runner may provide services this way, and `main` may need any of those its `Main` names; a package may provide another; without one, a pure `main` is called and an effect runs as on Node | — |
| `build.out-dir` | base directory of artifacts; not the project's root, the directory of its sources or tests, or one above them (`FLICK-P028`, before anything is written) | `"dist"` |
| `build.profile` | `"debug"` or `"release"` (see [codegen.md](../codegen/)); `--profile` of `build` and `test` overrides it | `"debug"` |
| `build.target` | default target backend short name (`"js"`, `"go"`, or the short name of a pinned `[build.backend]`); overridden by `--target <name>` on `build`, `run`, `dev`, `test`, `pack` and `inspect`. On `"go"`, Go must be on `PATH` and allowed in `[tools]` as `"flick/go" = ["go"]` (`FLICK-P029`), reachable uses of JS-only modules (`Ui`, `Atom`, `Action`, `Form`, `Otlp`, `DevTools`, `Js`, `Reactivity.*`, the `Node`/`Browser`/`Cloudflare` runners) or other gated modules report `FLICK-P064` in `flick check`, the editor and `flick build` (unused imports do not fail), and constructs the go target does not compile yet are `FLICK-P012` (see [Go target](../codegen/#go-target)) | `"js"` (or the backend prefix of `build.environment`) |
| `build.environment` | `<backend>/<host>` pair such as `"js/node"`, `"js/browser"`, `"js/bun"`, `"js/cloudflare"`, `"go/native"`, or a host provided by a `[host]` dependency package; resolves to the backend's active tags plus the host's tags. Unknown backends report `FLICK-P059`; unknown or incompatible hosts report `FLICK-P065`; unknown condition tags report `FLICK-P066`. `flick check`, the editor and `flick build` share reachable-use preflight: unavailable modules, packages or Effect-adapter bindings report `FLICK-P064` at the use site with the path from entry, and reachable gated effects without a root handler report `FLICK-E020`. Unused imports and unreachable functions do not fail; public library exports follow the export policy | the default environment of the selected backend |
| `build.solver-steps` | instance resolution steps per definition | `100000` |
| `build.solver-depth` | depth of nested class constraints | `256` |
| `build.labels` | what a release build labels `Module.name` for debugging tools: `"all"` (the values of every type marked `@label`: atoms, views, actions), `"components"` (those of the kind `component`: views and actions) or `"none"`; debug labels all; `flick build --labels` overrides it | `"all"` |
| `sql.dialect` | the SQL dialect of `sql table` and `sql { … }` (see [sql.md](../sql/)): `"postgresql"`; `"mysql"`, `"sqlite"`, `"clickhouse"` and `"oracle"` are recognized but not supported yet | `"postgresql"` |
| `lint.allow` | warning codes not reported, such as `"FLICK-T021"`; errors cannot be turned off | `[]` |
| `lint.no-let-assert` | `true` makes every `let assert` an error (`FLICK-T027`), in `flick check`, `build`, `test` and the editor | `false` |
| `lint.unused-proof-premises` | `true` warns about unused premises in checked proofs (`FLICK-T092`), in `flick prove` and the editor | `false` |
| `lint.max-function-lines` | lines of a function body past which `FLICK-T036` warns | `60` |
| `lint.max-nesting` | how deep `if`, `match`, `fun`, `do`, `task` and `for` nest before `FLICK-T037` warns | `4` |
| `lint.max-complexity` | branches of a function (cyclomatic complexity) past which `FLICK-T038` warns | `10` |
| `lint.max-params` | parameters of a function past which `FLICK-T039` warns | `5` |
| `plugins.<package>` | what the plugin of a dependency may reach for the project's blocks: `fs-read` (paths from the project's root), `net` (`host` or `host:port`) and `env` (names); not at a workspace root (see [Reaching outside](../plugins/#reaching-outside)) | nothing |
| `tools.<id>` | the host tools, `"node"` or `"go"`, that the build steps of the backend or plugin `<id>` may run, as `"flick/go" = ["go"]`; a step with another tool is `FLICK-P051`, before anything runs. The compiler finds an allowed tool on `PATH`, pins its digest and version, and runs each step in a directory beside the output with only the step's declared inputs and a cleared environment; the outputs are published with the rest of the build or not at all (`FLICK-P050`). A tool that only launches the program, as Node for `flick run`, needs no entry | nothing |

The same T036–T038 limits apply to `component`: setup and render are measured together in the source, without generated wrappers. Local functions are measured separately.

### Target options (`[target.<name>]`)

Backend-specific build and runtime options live in `[target.<name>]`, where
`<name>` is the backend's short name (`js` for `flick/js`, `go` for `flick/go`,
or the segment after `/` in a pinned `[build.backend]`). Multiple
`[target.<name>]` tables may coexist in one `flick.toml` for cross-target
projects; each invocation validates and applies only the table for its selected
target, and CLI flags override `flick.toml`. `[target]` is not allowed at a
virtual workspace root (`FLICK-P005`).

| Table and key | Values | Default |
| - | - | - |
| `target.js.typescript` | boolean; emit `.ts` instead of `.js` | `false` |
| `target.js.declarations` | boolean; emit `.d.ts` public TypeScript declarations | `false` |
| `target.js.kind` | `"app"` or `"library"`; `"library"` builds host-consumable library entry artifacts (`index.js`, `index.cjs`, or `index.iife.js`) plus `index.d.ts` from `@export "name"` functions without requiring app entries | `"app"` |
| `target.js.format` | `"esm"`, `"cjs"`, or `"iife"`; library output format (`cjs` and `iife` require `kind = "library"`) | `"esm"` |
| `target.js.global-name` | valid JavaScript identifier; global variable attached by `format = "iife"` | `"FlickLib"` |
| `target.js.promise-facade` | boolean; emit `A/B.promise.js` Promise façades | `false` |
| `target.js.schema-aot` | boolean; compile pure codecs ahead of use in entry runners (requires `"node"` in `[tools]`, or `--allow-tool node` outside a project) | `false` |
| `target.js.runtime` | `"rt"` on Flick's own runtime, or `"effect"` through the Effect adapter; a library a project depends on is compiled with the project's `runtime` | `"rt"` |
| `target.js.devtools` | boolean (`true` = `"ws://localhost:34437"`) or WebSocket URL string for `run` / `dev` | off |
| `target.js.ui` | module specifier rendering and mounting views in place of React (exports every name `Ui` and `Action` bind from `@flick-lang/effect-ts/ui` and `@flick-lang/effect-ts/ui/react`; relative paths are from the project root) | React |
| `target.go.cgo` | boolean; enable cgo (`CGO_ENABLED=1`) when building Go executables | `false` |
| `target.go.kind` | `"app"` or `"library"` (`"library"` is declared and currently refused by `flick/go` with `FLICK-P012`) | `"app"` |

`flick new` takes its template files from the selected target backend (`--target js` by default creates an RT project with `[tools] "flick/js" = ["pnpm"]`, `@flick-lang/runtime` and no Effect dependency; `--target go` creates a Go project with `[build] target = "go"`, `[tools] "flick/go" = ["go"]`, and no `package.json`), or from a third-party package via `--from`. Pass `--runtime rt` to `build`, `run`, `dev`, `test` or `inspect` to select RT for one invocation of an Effect project. Set `[target.js] runtime = "effect"` explicitly when a project needs the Effect adapter and add `@flick-lang/effect-ts` to its dependencies.

Legacy `[build]` keys (`emit`, `emit-dts`, `promise-facade`, `schema-aot`, `effect`, `ui`) and `[js]` / `[js.dependencies]` tables are removed with no compatibility aliases and fail with `FLICK-P057`, naming the replacement under `[build] target` or `[target.js]`:

| Removed key or table | Replacement |
| - | - |
| `[build] emit = "js"` | `[build] target = "js"` (or omit; `"js"` is default) |
| `[build] emit = "ts"` | `[build] target = "js"` and `[target.js] typescript = true` |
| `[build] emit = "go"` | `[build] target = "go"` |
| `[build] emit-dts = true` | `[target.js] declarations = true` |
| `[build] promise-facade = true` | `[target.js] promise-facade = true` |
| `[build] schema-aot = true` | `[target.js] schema-aot = true` |
| `[build] effect = "rt"` | `[target.js] runtime = "rt"` (or omit; `"rt"` is default) |
| `[build] effect = "effect"` | `[target.js] runtime = "effect"` |
| `[build] ui = "./renderer.mjs"` | `[target.js] ui = "./renderer.mjs"` |
| `[js.dependencies]` | `[target.js.dependencies]` |

Errors in the configuration exit with code `2`:

| Problem | Diagnostic |
| - | - |
| TOML syntax, an unknown key, a value of the wrong type or outside its set | `FLICK-P005` |
| a removed `[build]` key or `[js]` / `[js.dependencies]` table | `FLICK-P057` |
| an unknown or invalid key or value in `[target.<name>]` | `FLICK-P058` |
| an unknown `[build] target`, `[build] environment` backend, or `--target` name | `FLICK-P059` |
| an unknown or unsupported `--template` for `new` or `init` | `FLICK-P060` |
| an unknown host in `[build] environment`, a duplicate `[host]` declaration, or an unmet `[host] requires` condition | `FLICK-P065` |
| a `[package.abi]` version requirement incompatible with the active backend's published value ABI | `FLICK-F014` |
| a `schema-version` other than `1`, or none | `FLICK-P006` |
| `kind = "app"` without `entry` or `entries`, or with an empty `entries` | `FLICK-P007` |
| both `entry` and `entries`, or an entry name that is not one | `FLICK-P005` |
| no project found, or `--project` names none | `FLICK-P008` |
| a command needs a project at a workspace root, `--package` names no member, a member has no `flick.toml`, a path dependency is not a library, or dependencies form a cycle | `FLICK-P031` |
| an entry is not a source module that exports `main` | `FLICK-P011` |
| an entry `main` does not take and return `Unit`, needs a service other than the `Host` ones (with a runner, other than those its `Main` names) or can fail with an error other than `AppError` | `FLICK-P026` |
| `sql.dialect` names a dialect that is not supported yet | `FLICK-P022` |
| `sql.dialect` names no dialect; the message lists the known ones | `FLICK-P023` |
| `lint.allow` names an unknown code | `FLICK-P005` |

### Environments, host packages, and `when` conditions

Each backend component declares its base environment tags, selectable tag axes, and built-in hosts in its descriptor:

- `flick/js` publishes tag `js`, exclusive axis `runtime` (`rt` default, `effect`), additive axis `adapter` (`effect-adapter`, active when `@flick-lang/effect-ts` is in project dependencies), and built-in hosts `node`, `browser`, `bun`, and `cloudflare`.
- `flick/go` publishes tag `go`, additive axis `cgo` (`cgo`, active with `[target.go] cgo = true`), and built-in host `native`.
- Third-party backends can publish their own tags, axes, and hosts through the same descriptor schema.

`flick/lua` is a preview backend for World of Warcraft addons and LÖVE games.
It lives in the repository at `backends/lua` and is not distributed with the
compiler: pin its `package/component.wasm` in `[build.backend]`, or, with a
compiler built from the repository, select it with `--target lua`. It
publishes tag `lua`, exclusive axis `dialect` (`lua51` default, `luajit`),
additive axis `capabilities` (`jit` and `ffi`, each needing
`dialect = "luajit"`), and hosts `wow-retail`, `wow-forever` and `love`, each
pinned to one client or release. `flick check` and the editor check the
project's modules against the selected host and report what they reach but
the host cannot run, such as `BigInt`, `Int64` without `ffi`, suspension on
WoW or a global outside the host's API, with `FLICK-P012` at the use.

`flick build` writes the Lua kernel's chunks, `_flick/lua-kernel/*.lua`, and
a Lua chunk for each module the program reaches: `App/Util.lua` for a
project module and `_flick/List.lua` for a built-in one, each with a version
3 `.map` from its lines to the Flick source, and `chunks.txt`, the chunks in
the order they load. Each chunk starts with `local _, ns = ...` and puts its
module's values into the namespace it is given second, as WoW gives each
file of an addon; on LÖVE, `main.lua` loads the chunks in that order. A
top-level value is computed as its chunk loads. The kernel computes `Int`
arithmetic, `==` of any value, and the order and `++` of strings, whose
lengths and positions count UTF-16 code units as on the other targets; the
built-in modules run their Flick bodies where no `@native` clause names Lua.
A `foreign pure f from global "string.gsub"` is called directly when what it
is given is a string, a boolean, a number or a function of one pure stage
over those, and what it gives back a string, a boolean, a float or `Unit`.
Effects, other foreign functions, generators, what the kernel does not
compute yet (the other integer widths, `Float32`, `%` and `**` of `Float`,
`BigInt` and `Int64`) and a function over the VM's limits stop the build with
`FLICK-P012` at the project's code. A refused build keeps the previous
output. The host runs the program, so `flick run` and `flick dev` stop with
`FLICK-P012`. On `lua/love`, `flick test` runs the pure tests on Lua 5.1.5
or LuaJIT 2.1, whichever `[tools]` allows (`"flick/lua" = ["lua", "luajit"]`;
LuaJIT when both are), and reports as on the other targets; no WoW client
runs tests.

A library (`kind = "lib"`) may also declare `[host]` to define a named host environment `<backend>/<name>` and contribute extra tags when selected:

```toml
[project]
kind = "lib"

[host]
name = "worker"
backend = "js"
requires = "rt || effect"
tags = ["edge"]
```

- `host.name`: host name used in `[build] environment = "<backend>/<name>"` (automatically added as an active tag).
- `host.backend`: target backend short name or id (such as `"js"` or `"flick/js"`).
- `host.requires`: optional boolean condition evaluated against the backend's active tags; unmet requirements report `FLICK-P065` (exit code `2`).
- `host.tags`: additional environment tags contributed when this host is active.

Packages and modules restrict themselves to environments with boolean expressions over tags using `&&`, `||`, `!`, and parentheses:

- `[package] when = "<cond>"` in `flick.toml` gates every module of that package.
- `@when "<cond>"` immediately before `module` (or `private module`) gates a single module:

```flick
@when "js && !browser"
module App.ServerOnly exposing (ready)

ready : Bool
let ready = true
```

Every tag referenced in `@when`, `[package] when`, and `[host] requires` is checked against the combined vocabulary of known backends and loaded `[host]` packages; typos fail at the tag span with `FLICK-P066` (exit code `1`). Reachable uses from an app's entries and runners (or a library's active public exports) that enter a gated module, package, or Effect-adapter binding report `FLICK-P064` (exit code `1`) at the use site with the path from entry, identically across `flick check`, the editor, and `flick build`. Unused imports and unreachable functions never fail, and libraries skip public modules whose own condition is false in the selected environment.

## Entry points

An app's entries are the programs it builds: `flick build` writes a runner
`flick-<name>.js` next to the modules for each, and the modules once, so two
entries share what they import. A runner imports the entry's `main`, from the
module that defines it when the entry re-exports it, and starts it; on the JS
target it begins with `#!/usr/bin/env node`, so `flick/js`'s `pack` command
names each runner in the npm package's `bin`, and the only entry `main` under
the package's name. `flick run <name>` and `flick dev <name>` start one; with a
single entry the name may be left out.

Every entry exports `main : Unit -> Unit` or a runtime function taking and
returning `Unit`, requiring no services but those of `Host.Live` (with a
runner, those its `Main` names, such as `Env` of `Runner.Cloudflare`) and
failing only with `AppError` (an empty error row is valid too). A `main` started
without a runner gets the live implementations of the `Host` services its row
names (`Console`, `Clock`, `Random`, `Environment`, `Arguments`, `Process`)
from the launcher, which imports only those, so a `main` that names none
carries none; on the Go target it gets all six. What `main` provides itself
wins over them. The playground's are the same, with no environment variables
and no arguments, and a child process cannot start there (see
[who provides them](../services/#who-provides-them)). Provide other required
layers and handle other errors inside `main`, not in the launcher.

A pure `main` is called directly. Normally its launcher imports no runtime;
`[target.js] schema-aot = true` still imports the schema installation helpers.
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

The root may give every member a plugin, as a dependency and its grants:

```toml
[[workspace.plugins]]
package = "mysql"
source = { path = "plugins/mysql" }   # relative to the root, or a git or url source
grants = { fs-read = ["schema/"], net = ["localhost:3306"] }
```

Each member but the plugin's own package then depends on `mysql`, and grants it
the root's `grants` on top of its own `[plugins.mysql]`; `fs-read` is from the
member's directory. A member that names `mysql` from another source is
`FLICK-P005`. The plugin's member comes before the others, and `flick pack`
refuses a library that gets a plugin this way: its published manifest would not
have it, so it names it in `[dependencies]`.

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

A project sees these implicit source packages without naming them. Their
Flick module names stay the same: `Stream`, `HttpApi` and `Rpc` need no new
import spelling.

| Package | Source modules | Uses |
| - | - | - |
| `std` | values, effects, resources and concurrency | compiler core |
| `stream` | `Channel`, `Sink`, `Stream` and their parts | `std` |
| `process` | `ChildProcess` | `std`, `stream` |
| `ui` | reactivity, `Atom`, `Ui`, `Form`, `Action` | `std`, `stream` |
| `http` | `Http.*`, `HttpApi` and `Runner.Cloudflare` | `std`, `stream` |
| `rpc` | `Rpc`, `Server` and `Config` | `std`, `stream`, `http` |
| `observability` | `Otlp`, `Prometheus`, `DevTools` | `std`, `http` |
| `cli` | `Cli.Prompt`, `Cli` | `std`, `rpc` |
| `js` | `Js`, `Js.Array`, `JsError` | compiler core |

`Config` belongs with `Server` because secret configuration needs its
server-only capability. Source ownership does not select a runtime: a program
that uses only RT-supported bindings needs no Effect adapter. Stream and other
adapter-backed bindings still require `@flick-lang/effect-ts`; mixed programs
must match the installed adapter manifest's runtime ABI and exact dependency
versions. Existing target restrictions and plugin diagnostic codes still apply.

`flick install` fetches each package from the compiler release's
`std-v<version>` tag, with `subdir = "packages/<name>"`, and pins it in
`flick.lock`. An entry of that name in `[dependencies]` picks another source,
as the `std` above does. Keep the complete lock:
`flick install --locked --offline` reuses its installed packages without
updating pins or reaching the network. A compiler built from a checkout takes
that checkout's packages instead of the release. An incompatible implicit module is `FLICK-P043`.

For a source override, `FLICK_STD_PATH` names `packages/std`. The other
implicit packages must be its siblings, each with `src/modules.txt`.
To override packages individually, give each its own path, git or tarball entry
in `[dependencies]`.

The library's directory holds a `flick.toml` of `kind = "lib"`; its
`source-dir` is where the modules are, and its own dependencies come along.
A project declaring `[package]` (a library others fetch, or an app packed as a
Flick source tarball, including Go projects) says what it is and which compilers
build it, the last a semver requirement:

```toml
[package]
name = "text"
version = "1.0.0"
flick = ">=0.1, <0.2"

[package.abi]
"flick.javascript-ffi" = "0.1.0"
```

The optional `[package.abi]` table maps a backend value-ABI identifier (such as
`"flick.javascript-ffi"` or `"flick.go-ffi"`) to an exact semver version or semver requirement
(`">=0.1.0, <0.2.0"`). During `flick check`, IDE analysis, and `flick build`,
every active package (the root package and any dependency whose `[package] when`
condition holds) has its matching entry checked against the active backend's
published `descriptor.value-abi`; an incompatible version fails with
`FLICK-F014` at the manifest entry instead of issuing a silent foreign call.

A library that provides a plugin names its WebAssembly component, a path
inside the library (see [plugins.md](../plugins/)):

```toml
[package]
name = "pairs"
version = "1.0.0"
plugin = "pairs.wasm"
```

`watch = ["Ui.Event.queue", "*.parse"]` beside `plugin` names the calls its
plugin is asked about in every module checked (see
[plugins.md](../plugins/#checks-of-calls)); an entry that is no
`Module.name`, `Module.Sub.name` or `*.name`, or a `watch` without a `plugin`,
is `FLICK-P032`.

`load = ["proto"]` beside `plugin` says its plugin turns the files of those
extensions into modules: `App/User.proto` is the module `App.User` (see
[plugins.md](../plugins/#files-of-other-languages)). Two libraries that load
one extension are `FLICK-P045`. `hooks = ["emit"]` lets its plugin rewrite the
files `flick build` writes (see
[plugins.md](../plugins/#rewriting-what-a-build-writes)), and `hooks = ["ir"]`
the IR a build compiles (see [plugins.md](../plugins/#rewriting-the-ir)).

`flick install` fetches each git or url library the manifests name, directly
or through a library, into the package store (`FLICK_STORE`, else
`flick/pkg` in the user's cache directory), where each sits under the hash of
its files, and pins it in `flick.lock`: the commit of a git tag and that
hash. Commit `flick.lock`: a build reads the store through it and fetches
nothing. `flick install --offline` uses only the store, `--locked` fails
rather than change the lock, `flick update <name>` fetches a library again,
`flick add <name> --git URL --tag T [--subdir S]` (or `--url U --sha256 H`,
or `--path DIR`) adds one and `flick remove <name>` drops it.

The official backend a project builds with when it pins none (`flick/js`,
or `flick/go` with `[build] target = "go"` or `--target go`) is not part of the compiler either: it is a
release asset. `flick new`, the first build, or `flick install` takes the newest release
the compiler accepts from the backends index, keeps it in the package store
and pins it in `flick.lock` under `[[backend]]`; later builds work offline.
`flick update` moves the pin to a newer release. With `--offline` and no pin
in the store, or a component whose digest is not the pinned one, the build
fails with `FLICK-P056` and no other backend is used.

Projects created or initialized from a third-party template package with
`flick new --from` or `flick init --from` also record the verified template
source, tree checksum, component digest, and (for tarball URLs) archive digest
under `[[template]]` in `flick.lock`. `flick install` preserves `[[template]]`
entries without coupling `[dependencies]` to the template package.

A path library's plugin is built in place, so `flick.lock` also records the
hash of its component under `[[plugin]]`. A build is not held to it;
`flick install --locked` fails when the component is another build, and
`flick plugins` shows it.

`flick plugins` lists each plugin the project's manifests name, in the order
they name them: the project's `[dependencies]` and the `[[workspace.plugins]]`
a member inherits. A fetched library is shown by the source `flick.lock` pins,
not by its directory in the package store:

| source | `flick plugins` shows | pinned by |
| - | - | - |
| `path` | the component's path from the project's root | `[[plugin]]` in `flick.lock` |
| `git` | `git+<url>?tag=<tag>[&subdir=<dir>]#<commit> (<component>)` | the package's `[[package]]` checksum |
| `url` | `url+<url> (<component>)` | the package's `[[package]]` checksum |

The listing reads `flick.lock` and the package store and fetches or builds
nothing; a component a path library has not built is shown as missing. A
fetched dependency the lock does not pin, or the store does not hold, is
`FLICK-P034`. A library's own dependencies are compiled with it, but they are
not entries of the project's `flick.toml`, so the listing does not repeat them.

A library is compiled like a [source package](#packages): for the target the
project emits, without its warnings, after the project's own modules, as
another package, so instances of its classes and types are its own. A change
to a path library's sources or its `flick.toml`, or to `flick.lock`, rebuilds
the project, and `--watch` follows them.

### Foreign modules of a library

A module of a library binds a file beside it as a project's module does:
`foreign pad from "./pad.mjs" "pad"`. An `.mjs` or `.cjs` inside the library's
source directory ships with a js build: the output holds it beside the compiled
module, with the other `.mjs` and `.cjs` of the library it may import, so a JS
app packed by `flick/js`'s `pack` command has them and they find npm packages
from the project. A
file the `foreign` names that is not there, a link that leaves the library, or
two libraries with different files at one path are `FLICK-P041`. Any other
relative file, a `.js` or one outside the source directory, stays where it is
and the output refers to it there, as for the project's own modules, so it
ships with no tarball. On the go target the build copies `pad.go` beside the
same module into the Go module, and a helper it needs goes in that file.

### Target packages of a library

A project's own target-ecosystem dependencies stay in the backend's manifest
(for JavaScript, `package.json`). A library that needs target packages at run
time names them under `[target.<name>.dependencies]`; each backend defines and
validates its own syntax. For `flick/js`, entries under
`[target.js.dependencies]` are npm package versions or ranges from the
registry, not a path, a URL or a `workspace:` protocol:

```toml
[target.js.dependencies]
left-pad = "^1.3.0"
```

`flick install` and `flick update` gather `[target.<name>.dependencies]` from
the project's manifest and from each library it depends on, directly or through
another, check that declarations for the same `(target, package)` agree
(`FLICK-P040`), and pass the selected backend's slice to that backend. For
`flick/js` (with `[tools] "flick/js" = ["pnpm"]`), the backend adds what
`package.json` lacks to its `dependencies`, creating the file if the project
has none; `pnpm` then installs and pins them as the project's own, and
`flick/js`'s `pack` command ships them in the npm manifest of an app. `--locked` fails rather
than change `package.json`.

A js build without them fails with `FLICK-P034`, which says to run
`flick install`; a build for go ignores `[target.js.dependencies]` and needs
neither Node nor pnpm.

For `flick/go`, entries under `[target.go.dependencies]` declare Go module
paths pinned to canonical semantic versions (`vMAJOR.MINOR.PATCH`):

```toml
[target.go.dependencies]
"github.com/google/uuid" = "v1.6.0"
```

`flick/go` validates each module path and version (`FLICK-P032`), checks for
conflicts across packages and against any root `go.mod` (`FLICK-P040`), writes
the required modules into the generated program's `go.mod`, and generates (or
merges from a root `go.sum`) the program's `go.sum` during the Go build.

Two packages must name one target dependency with the
same text, and the backend's manifest (such as `package.json` or `go.mod`) with it, or the
build and `flick install` report `FLICK-P040`. The old `[js.dependencies]`
table is removed and fails with `FLICK-P057`.

### Packaging (`flick pack`)

`flick pack [--target <name>] [--out-dir <dir>]` produces up to two kinds of
release artifacts, after checking that no `todo` or `echo` remains in the
project (`FLICK-P024`) and warning on stderr if the public API breaks importers
since the latest git tag (`flick diff <tag>`):

1. **Flick source tarball (core, backend-neutral).** Whenever `flick.toml`
   declares `[package]` (`name` and `version`), core writes a deterministic
   `<name>-<version>.tgz` (normalizing `@scope/name` to `scope-name-<version>.tgz`)
   containing `flick.toml`, the project's `source-dir` (excluding test
   companions, `.git`, `.flick`, build outputs, and target package directories),
   any declared `[package] plugin` component, and root `README*` / `LICENSE*`
   files under `package/` with zeroed timestamps and fixed permissions. stdout
   prints `<sha256>  <path>` so a dependent project can install the tarball
   directly with `flick add <name> --url <url> --sha256 <sha256>`. A library
   inheriting `[[workspace.plugins]]` is refused (`FLICK-P032`), because those
   plugins would be missing from the published manifest.
2. **Optional backend ecosystem artifact.** If the selected target backend
   declares a `pack` capability, core runs a release build, verifies every
   emitted `.iface` file (`FLICK-P020`), and delegates target packaging to that
   backend inside `.flick/pack/`. For `flick/js`, an app with `package.json`
   and `[tools] "flick/js" = ["pnpm"]` stages a production `package.json`
   (without `devDependencies` or `scripts`, with `"type": "module"`,
   `"files": ["dist"]`, and entry `bin` fields), checks JS `.iface`/`.d.ts`
   companions and explicit `exports` (`FLICK-P020`; `[target.js] typescript = true`
   is refused), verifies the pinned `packageManager` (`FLICK-P016`), and runs
   `pnpm pack`. Libraries and JS apps with `[package]` and no `package.json`
   skip `pnpm` and emit only the Flick source tarball. When a backend artifact
   has the same filename as the Flick source tarball, core keeps the Flick
   source tarball at `<out-dir>/<name>-<version>.tgz` (matching stdout) and
   moves the backend artifact to `<out-dir>/<target>/<filename>`.

Backends without a `pack` capability (such as `flick/go`) run a target-aware
frontend check and write only the Flick source tarball without requiring Node
or pnpm; invoking `flick pack` on such a project without `[package]` fails with
`FLICK-P032`.

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

Without paths `flick check` checks every module of the source and test roots
except the test companions beside the sources (see
[testing](../testing/#test-companions)). With paths it checks those files and
directories, together with every module they import, transitively; of the
companions, only one that a path names. Outside a project paths are required.

## Packages

Target-ecosystem packages are discovered by the selected backend component,
not hardcoded in the compiler. For `flick/js`, npm packages live in
`package.json` and `pnpm` installs them; the JS backend inspects
`package.json` and `node_modules/<name>/package.json` and reports any package
whose manifest names Flick sources:

```json
{ "name": "@acme/contract", "flick": { "source-dir": "src" } }
```

Each dependent compiles its modules for the target it emits, js or go, as
modules of that package. Within a repository or across targets (such as a Go
server and a JS client), a [Flick dependency](#flick-dependencies) in
`[dependencies]` shares the library without needing `node_modules` or
`package.json` on the Go side. Its modules are checked for the target and
environment of the dependent, so on go a reachable use of a js-only module is
`FLICK-P064` (unused imports do not fail). The dependent does not report the
package's warnings. Its own
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
