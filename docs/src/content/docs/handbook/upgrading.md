---
title: Upgrading to 0.2
description: What changed since Flick 0.1.0, and how to update a project.
---

Before 1.0 a minor release may break programs, and 0.2 does: `std` is a
package the project fetches, the `cli`, `codec` and `server fn` blocks and
the derives of `Schema`, `Url`, `Form`, `HttpApi` and `Cli.Value` moved from
the compiler into the plugin of `std`, and libraries are compiled from their
sources. This page lists each change since 0.1.0 and how to update a project.

## How to upgrade

Build the compiler from a checkout of `main` (see
[Getting started](../../start/getting-started/#from-source)); it takes the
`std` of the same checkout (see [the next section](#std-comes-from-a-package)).
Then run in the project:

```sh
flick install
flick check --fix
```

`flick check --fix` applies the safe fixes: it rewrites `schema T` and
removes the imports of plugin heads every module has, along with the other
fixes it always applies. It reports what is left, and the sections below say what each
change needs.

| Change | `flick check --fix` | What you see |
| - | - | - |
| [`std` comes from a package](#std-comes-from-a-package) | no: run `flick install` | `FLICK-P034` |
| [`schema T` is `T.schema`](#schema-t-is-tschema) | yes | `FLICK-T001` |
| [Blocks of `std` need no import](#blocks-of-std-need-no-import) | yes | `FLICK-R012` |
| [Diagnostic codes under `std/`](#diagnostic-codes-under-std) | no | new codes |
| [`deriving HttpApi` is stricter](#deriving-httpapi-is-stricter) | no | `std/D011`, `FLICK-C001` |
| [SQL queries are stricter](#sql-queries-are-stricter) | no | `std/Q010`, `FLICK-T003` |
| [What may suspend says `Async`](#what-may-suspend-says-async) | yes: `--only E002,F001,T003` | `FLICK-E002`, `FLICK-F001`, `FLICK-T003` |
| [Libraries are compiled from sources](#libraries-are-compiled-from-sources) | no | `FLICK-R003` |
| [A `tail` arm may not suspend](#a-tail-arm-may-not-suspend) | no | `FLICK-E018` |
| [`Scope.current` is not exposed](#scopecurrent-is-not-exposed) | no | `FLICK-T001` |
| [Work a region keeps holds no `control` effect](#work-a-region-keeps-holds-no-control-effect) | no | `FLICK-E017` |
| [The IR has no `Try`, `Provide`, `Scope` or `Operation`](#the-ir-has-no-try-provide-scope-or-operation) | no | — |
| [The IR carries lexical owner evidence](#the-ir-carries-lexical-owner-evidence) | no | — |
| [Instances belong to one package](#instances-belong-to-one-package) | no | `FLICK-C006` |
| [Plugins reach only what is granted](#plugins-reach-only-what-is-granted) | no | `FLICK-P038`, `FLICK-P039` |
| [`LogLevel` is an enum](#loglevel-is-an-enum) | no | `FLICK-T001` |
| [Plugins follow the compiler's interface](#plugins-follow-the-compilers-interface) | no | — |
| [`CallSite` replaces the `_at` writes](#callsite-replaces-the-_at-writes) | no | `FLICK-T001` |
| [Regions are checked by use](#regions-are-checked-by-use) | no | `FLICK-G002`, `FLICK-G003` |
| [Rust 1.95 builds the compiler](#rust-195-builds-the-compiler) | no | — |
| [`mut` is a keyword](#mut-is-a-keyword) | no | `FLICK-S007`, `FLICK-S016` |
| [Children in `ui` stand one by one](#children-in-ui-stand-one-by-one) | yes: `--only T073` | `FLICK-T073`, `FLICK-T003` |
| [`refined type` is a declaration of std's plugin](#refined-type-is-a-declaration-of-stds-plugin) | no | `FLICK-P036`, `FLICK-R005` |
| [Braces after `then` and `else` hold statements](#braces-after-then-and-else-hold-statements) | yes: `--only T074` | `FLICK-T074` |
| [No build without a runtime](#no-build-without-a-runtime) | no | `FLICK-P005` |
| [A provided instance leaves the body no other](#a-provided-instance-leaves-the-body-no-other) | no | `FLICK-E019` |
| [Backends find the built-in types by lang item](#backends-find-the-built-in-types-by-lang-item) | no | `FLICK-S014` |
| [`Detached` forbids `abort`; `lazy` uses `NoControl`](#detached-forbids-abort-lazy-uses-nocontrol) | no | `FLICK-E017`, `FLICK-F002` |
| [Backend options move to `--target` and `[target.<name>]`](#backend-options-move-to---target-and-targetname) | no | `FLICK-P057`, `FLICK-P058`, `FLICK-P059` |
| [Target dependencies move to `[target.<name>.dependencies]` and backends own package managers](#target-dependencies-move-to-targetnamedependencies-and-backends-own-package-managers) | no | `FLICK-P051`, `FLICK-P057`, `FLICK-P063` |
| [The IR carries full layout types (`0.14.0`)](#the-ir-carries-full-layout-types-0140) | no | — |
| [Backends declare foreign adapter metadata (`0.15.0`)](#backends-declare-foreign-adapter-metadata-0150) | no | `FLICK-S011`, `FLICK-S014` |
| [Fixed-width numeric types in IR `0.16.0`](#fixed-width-numeric-types-in-ir-0160) | no | — |
| [Inline foreign bindings and module preambles (`0.17.0`)](#inline-foreign-bindings-and-module-preambles-0170) | no | `FLICK-F011` |
| [Versioned backend value ABI tables](#versioned-backend-value-abi-tables) | no | `FLICK-F013`, `FLICK-F014` |
| [`FuncRef` and callback retention in IR `0.18.0`](#funcref-and-callback-retention-in-ir-0180) | no | `FLICK-F015`, `FLICK-F013` |
| [Host exports and library build artifacts (`0.19.0`)](#host-exports-and-library-build-artifacts-0190) | no | `FLICK-F004`, `FLICK-P012`, `FLICK-S014` |
| [Plugin protocol `0.5.0`](#plugin-protocol-050) | no | `FLICK-P035`, `FLICK-P036`, `FLICK-P044` |
| [The kernel's defects](#the-kernels-defects) | no | — |
| [`Math.log` in the kernel, and an exact `Float.next_pow2`](#mathlog-in-the-kernel-and-an-exact-floatnext_pow2) | no | — |
| [Hot-swap compatibility and declared state-cell migrations (formal model)](#hot-swap-compatibility-and-declared-state-cell-migrations-formal-model) | no | — |
| [VM Tier 1 runs std and CLI intrinsics](#vm-tier-1-runs-std-and-cli-intrinsics) | no | — |

`flick check --fix page.md` rewrites the `flick` code blocks of a Markdown
page, each checked on its own.

## `std` comes from a package

In 0.1.0 `std` was built into the compiler. Now the compiler holds only the
core modules, and `std` is a package. A project that does not name `std` in
`[dependencies]` depends on the `std-v<version>` tag of its compiler's
release, which `flick install` fetches and pins in `flick.lock`. Commit
`flick.lock`. Until `flick install` has run, every command that checks
modules stops:

```text
error[FLICK-P034]: `std` from `git+https://github.com/bondiano/flick-lang?tag=std-v0.1.0&subdir=packages/std` is not installed: run `flick install`
```

A compiler built from a checkout takes the `std` of that checkout instead,
while the checkout stays where it was built, so it needs no `flick install`
for `std`. A `std` written for another compiler stops every command with
`FLICK-P043`, which names the module that does not check. Then name the `std`
of the compiler's own source:

```toml
[dependencies]
std = { path = "../flick-lang/packages/std" }
```

The standard library uses several [implicit source packages](../../reference/project/#flick-dependencies).
Run `flick install` again and commit the updated `flick.lock`, so fetched
packages are pinned together. Then
`flick install --locked --offline` uses the installed set. Public module names
and the logical `std` plugin remain the same, including its diagnostic codes.
A project that uses only RT-supported bindings still needs no adapter; a mixed
project keeps the adapter version and runtime ABI its manifest requires.

For a local override, set `FLICK_STD_PATH` to `packages/std` of a complete
checkout, keeping the other implicit packages beside it with their own
`src/modules.txt`. A copied `std` directory alone no longer provides the whole
standard library. Alternatively, add a path entry like the one above for each
package you override; unspecified packages retain their release source.

## `schema T` is `T.schema`

A type's witness is its companion `T.schema`, and `schema` is no keyword
any more. In 0.1:

```flick fail T001
type alias Profile = { email : String }
  deriving Schema

profile_schema : Schema Profile
let profile_schema = schema Profile
```

Now `schema Profile` is a call of an unknown `schema`, `FLICK-T001`, and its
fix writes `Profile.schema`:

```flick
type alias Profile = { email : String }
  deriving Schema

profile_schema : Schema Profile
let profile_schema = Profile.schema
```

`flick check --fix` rewrites it.

## Blocks of `std` need no import

`cli`, `codec`, `server fn` and `sql` blocks (queries, `sql table` and
`sql migration`) are the plugin of `std` now, and every module has their
heads: a block needs no import, as in 0.1.

```flick
type alias Profile = { email : String; nickname : Option String }
  deriving Schema

profile_json : PureCodec Json Profile
let profile_json =
  codec Profile.schema {
    email = field "email_address" String.json;
    nickname = field "nickname" (option_as_null String.json);
  }
```

An import of such a head, `import Codec exposing (plugin codec)`, is
`FLICK-R012`, and `flick check --fix` removes it. Only a block without
`{`, such as `cli Main`, still needs the import. Another plugin may not give
these heads: its import of one is `FLICK-P044`.

`cli`, `codec`, `server` and `fn` are no keywords any more: they start a
block before `{` or at the start of a declaration, as `server fn` and
`sql table`, and are ordinary names elsewhere. `plugin` is a keyword only in
the exposed items of an import.

## Diagnostic codes under `std/`

A plugin reports under codes of its own, shown under its package. The rules
of the blocks and derives that moved keep their meaning under new codes:

| 0.1 | Now | What it reports |
| - | - | - |
| `FLICK-L001` … `FLICK-L007` | `std/L001` … `std/L007` | the rules of a `cli` block |
| `FLICK-S007` in a `cli` block | `std/L009` | a malformed clause |
| `FLICK-C013` of `deriving Cli.Value` | `std/L010` | a type `Cli.Value` cannot derive |
| `FLICK-D001`, `FLICK-D009`, `FLICK-D010` of `deriving` | `std/D001`, `std/D009`, `std/D010` | a type `Schema`, `Url` or `Form` cannot derive, on the class in `deriving` |
| `FLICK-D002` … `FLICK-D005`, `FLICK-D007` | `std/D002` … `std/D005`, `std/D007` | the rules of a `codec` block |
| a type mismatch in a `codec` block | `std/D012` | a field whose codec decodes another type than the record's |
| `FLICK-D011` | `std/D011` | a record that `deriving HttpApi` cannot read as endpoints |
| `FLICK-E010` | `std/E010` | the shape of a `server fn`; `server name` of a value that is no `server fn` stays `FLICK-E010` |
| `FLICK-Q001`, `FLICK-Q010` in a `sql table` | `std/Q001`, `std/Q010` | a column given twice, a SQL type or attribute the dialect does not know |
| `FLICK-Q011` | `std/Q011` | a column of another type than its SQL type's, without `via` |
| `FLICK-Q010`, `FLICK-Q013` in a `sql migration` | `std/Q010`, `std/Q013` | a statement outside the DDL the migrations hold, one that does not apply to the schema before it |
| `FLICK-Q001` … `FLICK-Q010` in a query | `std/Q001` … `std/Q010` | the rules of `sql { … }`, with the same texts |
| `FLICK-Q012` | `FLICK-C001` | a value whose type has no SQL form: no instance of `Sql.Param` for it |
| a mismatched capture beside a base column, `sum` or `avg` of a column with a codec | `FLICK-C001` | no instance of `Sql.Param` or `Sql.Numeric` for the type |
| `FLICK-E012`, `FLICK-L008` | — | retired: every build has the plugin of `std` |

`flick explain std/D001` explains a new code, and `flick explain FLICK-L001`
says the old one is retired and names `std/L001`. Update the scripts and CI
filters that match the old codes; the code itself needs no change.

## `deriving HttpApi` is stricter

The plugin of `std` derives the same endpoints, routes and documents, with
three differences:

- A value named after a type in its module, `credentials` for
  `Credentials`, is the codec of a part or a success only when it is a
  `PureCodec Json` of that type. Otherwise the JSON codec of the type's
  schema is used, and without one the part is `std/D011`.
- The check of a group of `Authed` and `MaybeAuthed` endpoints gets only the
  request's headers, `HttpApi.Request Unit Unit Credentials Unit`.
- An error without a `Problem` instance, and a path whose `:name`
  parameters are not the fields of its `path` part, are `FLICK-C001`
  (they were `FLICK-D011`).

These need a manual change.

## SQL queries are stricter

The plugin of `std` writes the same SQL text, with three differences:

- `from x in T` reads a table by its name only: a `sql table` or a table of
  migrations, qualified or not. Any other expression after `in` is
  `std/Q010`; name the table itself.
- Beside a column with a codec, a capture has the column's own type, never
  an `Option` of it (`FLICK-T003`). Test for `None` in Flick and build the
  query without the filter there.
- An `Option` of a capture reaches the row only when the capture is
  selected itself, as in `select { tag = $tag }`. Elsewhere it makes no
  field of the row an `Option`, and code that matched on one is
  `FLICK-T003`.

These need a manual change.

## What may suspend says `Async`

A function that waits for time, input or output or another fiber has the
effect `Async` in its row ([Effects](../../reference/effects/#what-may-suspend-async)).
In `std` the functions that wait have it: `Effect.sleep`, `Fiber.join`,
`Queue.take`, the operations of `Database`, `FileSystem` and `Terminal`,
`Http.Client`, the servers and clients of `Http`, `Rpc` and `HttpApi`. The
tasks of actions, events, atoms and tests take it, `Test.suite` takes a
layer that may need it, and the `main` of a runner may have it.

A `foreign` takes no `sync` or `async` word any more: without a mode word
its signature tells, and a row with `Async` is a promise.

```sh
flick check --fix --only E002,F001,T003
```

removes those words and gives the row of each `foreign async` its `Async`,
then adds `Async` to the rows that call what waits, pass after pass up to
`main`, and to the types written with such a
row, as `Cli.Command {Async, File.FileSystem} {AppError}`. It changes
nothing else. One case needs a hand: a function given rows of both kinds in
a position no subtyping reaches, as `Channel.Channel none …` given a channel
of a queue, takes a row variable instead: `Channel.Channel r e … ->{r; e} …`.

## Libraries are compiled from sources

In 0.1.0 an npm package gave its Flick modules as compiled interfaces:

```json
{ "name": "@acme/text", "flick": { "iface-dir": "iface" } }
```

Now a project compiles every Flick library from its sources, and
`iface-dir` is not read: an import of the package's modules is
`FLICK-R003`, unknown module. The package ships its sources instead:

```json
{ "name": "@acme/text", "flick": { "source-dir": "src" } }
```

or it becomes a Flick library that the project names in `[dependencies]`
by path, git tag or tarball (see
[Flick dependencies](../../reference/project/#flick-dependencies)).
`flick pack` of a library now writes a tarball of its sources and needs
`[package]` with `name` and `version` (`FLICK-P032` without them); an app
still packs an npm package with its runners. This is a change for the
library's author.

## A `tail` arm may not suspend

A `tail` arm runs on the stack of the caller, which may be Sync, so nothing it
calls may suspend. A call in it that needs `Async`, an effect with a `control`
operation or a row variable is now `FLICK-E018`:

```flick fail E018
# effect Ask =
#   ask : Unit ->{} Int
#
# pause : Unit ->{Async} Unit
# foreign pause from "./pause.mjs" "pause"
#   arity 0
#
waits : (Unit ->{Ask; none} Int) ->{Async; none} Int
let waits body =
  try body () with
  | Ask.ask () ->
    pause ()
    1
```

Declare what the arm needs in the row of the operation, as below, or make the
operation `control`. A handler inside the arm may catch such an effect too.

```flick
# pause : Unit ->{Async} Unit
# foreign pause from "./pause.mjs" "pause"
#   arity 0
#
effect Ask =
  ask : Unit ->{Async} Int

waits : (Unit ->{Ask, Async; none} Int) ->{Async; none} Int
let waits body =
  try body () with
  | Ask.ask () ->
    pause ()
    1
```

`flick check --fix` does not rewrite it: the operation and its callers change
together.

## `Scope.current` is not exposed

`Scope.current` gave the owner around under any region, so work could stay under
an owner longer than the region the type of that owner names, and a child could
use a resource after its release. It is the compiler's now, and a call is
`FLICK-T001`. Take the owner from `scope s { … }`, or as a parameter of type
`Fiber.Owner s`.

## Work a region keeps holds no `control` effect

`Scope.add_finalizer`, `Resource.acquire_release`, the listeners and the servers
now need `Detached r`, as `Fiber.spawn` does. A finalizer that performed a
`control` operation stopped there without an error once a handler let the body
go. It is `FLICK-E017` now: handle the effect inside the work. A function that
passes on a row variable names `Detached r` in its signature, and `flick check
--fix` does not add it.

## The IR has no `Try`, `Provide`, `Scope` or `Operation`

A reader of `flick build --emit-ir` meets a new version, `0.7.0`. A `try` with
arms only for errors is a `Handle`, `provide layer { body }` is a call of
`Layer.run`, `scope s { body }` is a call of `Scope.run`, and an operation is
`Perform { effect, op }`. A `let mut` variable is a `Stmt::Var`, its assignment
a `Stmt::Assign` and a read of it an `ExprKind::Var`. Sequential loops use
`ControlLoop`; `LoopExit` records the target and the optional break value.
Source code does not change.

## `mut` is a keyword

`let mut name = value` declares a variable of a function, which `name = value`
assigns (see [Variables](../../reference/bindings/#variables)). The word `mut`
is a keyword now: a parameter, a value or a record field named `mut` is
`FLICK-S007`, or `FLICK-S016` for `let mut = …`; rename it. After a dot,
`r.mut` is still a name.

`break` and `continue` are also reserved for local loop control; rename bare
bindings with these names. `loop` and `while` are contextual words, so existing
names such as `Loop.loop` and `Schedule.while` keep working.

## Children in `ui` stand one by one

Inside `ui { … }` the children of an element are no longer a list literal:
each is an argument of the call, on its line or on a line of its own under
it (see [Elements](../../reference/ui/)).

```flick
# view : Float -> Ui
# let view n = ui {
#   let shown = show n
// before: div { class_name = "counter" } [heading [text shown], spacer []]
  div { class_name = "counter" }
    heading (text shown)
    div { class_name = "spacer" }
# }
```

`flick check --fix --only T073` rewrites every list literal written the old
way. What it leaves:

- **A list among other children** (`[header] ++ rows` written as two
  arguments) is `FLICK-T003`: join the list first, or give a node for each
  item with `each rows row`.
- **A call broken over lines** inside `ui` reads each line under it as one
  argument. `View.classed "name" (text "x")` written on three lines still
  works; a line holding two arguments, `text "x"` without parentheses, is
  now one child.
- **`div props` inside `ui`** is the element with no children, not a
  function still waiting for them. Take the function outside `ui`, or write
  `fun children -> div props children`.

`ui` is no longer a reserved word: it starts a block before `{` and is a
name elsewhere. Outside `ui` nothing changes: `Ui.div props [a, b]`.

## `refined type` is a declaration of std's plugin

`refined type Port = Float where fun n -> n > 0` is written as before, and
the plugin of `std` now gives its code: the type, `Port.parse`, `Port.value`
and the predicate as `Port.check : Float -> Bool`. What changes for a
program:

- `refined` is no reserved word: a value may be named `refined`.
- `Port.check` is a name of the module. A module that declared its own
  `Port.check` beside the type has two.
- A declaration that lacks a part is `FLICK-P036` at the place it stops,
  where it was a syntax error.
- A class in the `deriving` of a refined type takes no arguments.
- A program with refined types no longer needs Effect for them: `parse`
  is code of the language, not a function of the runtime.

Built JavaScript and Go change for these declarations and nothing else;
`flick expand` shows the code.

## Braces after `then` and `else` hold statements

Braces right after `then` or `else` are a block of statements, as after `do`
and `loop`, so a branch can assign several variables:

```flick
# ordered : Int -> Int -> (Int, Int)
# let ordered a b =
#   let mut low = a
#   let mut high = b
  if low > high then {
    low = b
    high = a
  }
#   (low, high)
```

A record right after `then` or `else` now goes in parentheses,
`then ({ r with done = true })`. Written the old way it is `FLICK-T074`, and
`flick check --fix --only T074` adds the parentheses.

## Instances belong to one package

A library is another package for the checker, a path dependency too. Only
the package of a class, or of the type at the head of an instance, may
declare the instance. In 0.1.0 a project could declare an instance of its
path library's class for that library's type; now that is `FLICK-C006`,
orphan instance. Move the instance into the library that owns the class or
the type, or wrap the type in one of the project's own:

```flick
# type Id = Id String
# class Label a =
#   label : a -> String
#
type LocalId = LocalId Id

instance Label LocalId =
  label : LocalId -> String
  let label wrapped =
    match wrapped with
    | LocalId (Id raw) -> raw
```

## Plugins reach only what is granted

Plugins are new in 0.2. A plugin of a dependency reads files, variables of
the environment and hosts over HTTP only as the project grants in
`flick.toml`:

```toml
[plugins.pairs]
fs-read = ["schema/"]
net = ["api.example.com"]
env = ["API_TOKEN"]
```

Anything else is `FLICK-P038`, naming the line to add. Answers over HTTP
are kept under `.flick/plugin-snapshots/`, which the project commits. With
`--offline`, now an option of every command, or with `CI` set, a request no
snapshot answers is `FLICK-P039`. The plugin of `std` reaches nothing
outside, so a project that uses only `std` grants nothing. See
[Reaching outside](../../reference/plugins/#reaching-outside).

## `LogLevel` is an enum

Enums are new in 0.2: `untagged type Method = Get = "GET" | Post = "POST"`
is a type whose values are their literals (see
[Code generation](../../reference/codegen/)). `LogLevel` is one, its
constructors declared from least to most severe:

| 0.1.0 | 0.2 |
| - | - |
| `LogLevel.all`, `trace`, `debug`, `info`, `warn`, `error`, `fatal` | `LogLevel.All`, `Trace`, `Debug`, `Info`, `Warn`, `Error`, `Fatal` |
| `LogLevel.none_` | `LogLevel.Off`, whose literal is `"None"` |
| `LogLevel.values` | `LogLevel.all`, in order of severity |
| `LogLevel.name level` | `LogLevel.value level` |
| `LogLevel.get_ordinal a < LogLevel.get_ordinal b` | `compare a b`, the derived `Ord` |

`show` of a level gives its constructor, `Off` where 0.1.0 printed `None`.
Each old name is `FLICK-T001`; replace it by hand.

## Plugins follow the compiler's interface

A derive of an enum gets the literals of its constructors, and a plugin's
`check` the base type of an enum; a derive knows which constructors are of
a GADT (`refined`, `constrained` of a case), and a plugin asks while it
expands whether a type is an enum (`literal-of`). Rebuild a component with
the `flick-plugin` of the compiler it runs in.

## `CallSite` replaces the `_at` writes

The core class `CallSite` gives a function the place of its use (see
[Call sites](../../reference/classes/#call-sites)). `Atom.set`, `update` and
`modify` take it themselves, so `Atom.set_at`, `update_at` and `modify_at`
are gone (`FLICK-T001`): call `Atom.set atom value`, or give your own
function `CallSite =>` to report its caller's place. A serializable atom's
key is still `Module.name`, now its `CallSite.definition`.

## Regions are checked by use

The checker asks where a value **uses** a region, not what it **holds** (see
[Regions and captures](../../reference/regions/)). A closure, a task or a
constructor may now keep a handle past the end of its `scope` while nothing it
runs uses the handle there, so programs that were `FLICK-G001` for that alone
are accepted. A closure that reads a handle after its scope is `FLICK-G002`,
where it was `FLICK-G001`. A later stage of a curried function holds nothing
of its earlier parameters; it needs live what they reach, which the checker
reads into the signature.

- **`.iface` files** of the earlier discipline are rejected: their version is
  now 12. Rebuild the libraries.
- **A component** may use its instance's region in its setup again: `Setup s`
  keeps `s` live. Work that `Ui.on_mount` and `Ui.watch` start must use only
  what outlives the instance (`FLICK-G003`); `Lifecycle` now names the
  instance's region, `Lifecycle s`.
- **A handler a program installs** for a service a region indexes needs the
  region live where it is installed (`FLICK-G002`).
- **`Option.map`** returns `(Option b)^c`: what the callback returns holds
  nothing of its input.

## Data that stores a function holds what its parameters reach

Data that stores a function or a `task`, such as `type G s a = G (s -> a)`,
written bare in a signature now has its hidden row read as `Live` is: in a
parameter any row, in the result what the parameters before it reach, and
so in the stages of a parameter that is a function (see
[Regions and captures](../../reference/regions/#qualifiers)).

- **A callback that returns such data** holds what its own parameters reach:
  `(File s -> G String) -> …` lets the callback return a closure over the
  file, and what the outer function returns then holds the file's region.

- **A function that returns fresh data** after a parameter that reaches a
  region holds that region: `make : Handle r -> G Unit Int` cannot leave the
  `scope` of `r` (`FLICK-G001`). Write `(G Unit Int)^{}` when it holds
  nothing.
- **A callback taken out of such data** uses the data's row, so passing it
  to a callback parameter written bare is `FLICK-G002`: take the data
  itself, or write the row on the callback parameter.
- **`.iface` files** of version 20 are rejected: rebuild the libraries.

## Rust 1.95 builds the compiler

A checkout of `main` builds with Rust 1.95 (0.1.0 took 1.94); its
`rust-toolchain.toml` names the version for rustup. The `server-stdlib`
Cargo feature is gone: every build has the plugin of `std`.

## No build without a runtime

`[build] effect = "none"` is gone: `flick.toml` with it is `FLICK-P005`.
Remove the key, or write `effect = "rt"`: a program that runs nothing at
runtime builds on Flick's own runtime, which bundles only the helpers the
program calls. A library no longer writes `_core/` beside its modules, and
its `.iface` files have no `abi.core`; their version is now 22, so rebuild
the libraries. `FLICK-E008` is no longer reported.

## A provided instance leaves the body no other

A handler finds a service by its name, so `provide` of a layer of `Store Int`
answered every `Store` in its body, also a `Store String` a caller passed in
through a row variable: the checker accepted that, and the program got an `Int`
where it said `String`. Such a body is now `FLICK-E019`. Take the service out of
the variable, `Eff {-Store | r} …` (`-` now takes a service with arguments out by
its name), or name the instance the body needs, `Eff {Store Int | r} …`.

## Backends find the built-in types by lang item

A reader of `flick build --emit-ir` meets a new version, `0.9.0`: the program
has `lang`, where its modules declare each lang item, as
`{ "lang": "option", "module": "Prelude", "namespace": "type", "name": "Option" }`.
A backend finds `Option`, `Some` or `Int` there and not by name, which the
library may change. A plugin with `hooks = ["ir"]` gives the table back as it
got it; a changed one is `FLICK-P047`.

The built-in types carry no `@ts`: the JS backend gives them their TypeScript
forms. A package's `@ts` has one form, the same on every runtime, and imports
from a module by its specifier: `rt "…"` after a form or an export, and
`@ts import runtime` or `ui`, are `FLICK-S014`. `.iface` files of version 23
are rejected: rebuild the libraries.

## A module of the IR names its package

A reader of `flick build --emit-ir` meets a new version, `0.10.0`: a module has
`package`, the package it belongs to (the project's own modules take the name in
its `package.json`), which the backends write into the tags of its
constructors. A built-in module and one of a project without a name have none.
A plugin with `hooks = ["ir"]` gives the packages back as it got them; a changed
one is `FLICK-P047`.

## Plugin protocol 0.3.0

Rebuild external native plugins and WASM components with this compiler's
`flick-plugin` SDK. Components using protocol 0.2.0 now report `FLICK-P035`
instead of loading against incompatible component types.

Derive inputs contain parsed types: `CaseTarget.fields` is `Vec<Written>`,
and `DeriveTarget.alias` and `wraps` contain `Written` instead of spans.
Read `Written.kind` to inspect a type and `Written.span` for its diagnostic
or hole location; do not reparse the declaration's token stream.
`Signature.ty` supplies the same parsed representation. Parameter names and
token-normalized kinds are available in `DeriveTarget.params` and `kinds`.
See [Derives](../../reference/plugins/#derives) for the input contract.

## A module of the IR lists its exports

A reader of `flick build --emit-ir` meets a new version, `0.11.0`: a module has
`exports`, what its interface exports. `values` lists the module's own exported
values, as `{ "name": "parse", "sync": 2 }`: `sync` is the number of arrows a
call supplies when the last stage is Sync, else `null`. `reexports` holds what it exports of other modules, as
`{ "module": "Http.Url", "namespace": "type", "name": "Url" }`. The JS backend
writes the module's `export … from` lines and the wrappers of its Sync exports
from them. A plugin with `hooks = ["ir"]` gives the exports back as it got
them; changed ones are `FLICK-P047`.


## The IR carries lexical owner evidence

Typed IR `0.12.0` adds `ReadOwnerEvidence` and `WithOwnerEvidence`. A read
observes inert lexical evidence at execution; it must not be hoisted across
an invocation or task. `WithOwnerEvidence` changes that evidence for an
immediate runtime application or await, and restores it on every exit. Its
invocation operands are immutable locals or literals, already evaluated
outside the node in source order. Unit means absent evidence, not a root
owner. This does not change the default owner for work or cancellation, or
the ordinary argument counts of functions and tasks.

Backends and IR plugins must preserve these nodes instead of turning them
into an ordinary `Within`: transporting evidence neither validates nor
reopens a region. Actual owner use still performs the lifetime check.
`Foreign.source_name`, when present, records the original declaration name
of a compiler-generated foreign shim; its host module/export binding remains
unchanged. Rebuild cached interfaces: the compiler now reads `.iface` schema
26 and rejects older schemas, including 24 and 25. Schema 26 stores required
non-public type definitions separately from the public export namespace; it
does not make those names importable. The separate experimental memory IR is not a new
field of the serialized typed program.

## Library acquisition regions

`RcRef.make`, `RcMap.make`, `Pool.make` and `Pool.make_with_ttl` quantify a
fresh region for each acquisition or lookup, as `ScopedRef` and `ScopedCache`
already do. The library supplies that region when the callback runs and
releases its resources after the last borrower finishes. Acquisition cannot
store a capability of its private region in an outer `Ref`; this reports
`FLICK-T007`. A separately bound polymorphic task remains a valid acquisition.
The container's owning region and the callback's ordinary service requirements
remain independent of the private acquisition region.

## `Detached` forbids `abort`; `lazy` uses `NoControl`

`Detached r` now requires that no effect in `r` has either a `control` or an
`abort` operation (`FLICK-E017`). Previously only `control` was rejected, so an
`abort` inside a child fiber (`Fiber.spawn`, `parallel`, `race_success`,
`Effect.cached`, concurrent `Effect.for_each` / `partition` / `validate` /
`filter` / `filter_map_effect`, `Effect.timeout_or_else`) or a scope finalizer
could reach a kept handler from outside its installing fiber. Handle `abort`
effects inside the child or finalizer, or use typed errors (`type error`) and
`join` / `Channel` to report outcomes back to the parent.

A `lazy` initializer still allows `abort` because it runs synchronously in the
forcing fiber; `lazy` and `lazy_make` now require the superclass `NoControl r`
(`class NoControl r => Detached r`), so every `Detached r` constraint still
satisfies `NoControl r`. Foreign declarations with `retention owner` and more
than one region must also include `Detached r` alongside `Fiber.Provided r s`
(`FLICK-F002`). Typed IR advances to `0.13.0` to include the `no_control` lang
item.

## Backend options move to `--target` and `[target.<name>]`

Target-specific CLI flags and `[build]` keys have been replaced by backend-declared
options with no compatibility aliases. Every removed flag or `[build]` key fails
with `FLICK-P057` (exit code `2`) naming its exact replacement:

### CLI flags

| Removed | Replacement |
| - | - |
| `flick build --emit js` / `flick test --emit js` | `--target js` (or omit; `js` is default) |
| `flick build --emit ts` | `flick build --target js --typescript` (or `[target.js] typescript = true`) |
| `flick build --emit go` / `flick test --emit go` | `--target go` (or `[build] target = "go"`) |
| `flick build --emit-dts` | `flick build --declarations` (or `[target.js] declarations = true`) |
| `flick build\|run\|dev\|test --rt` | `--runtime rt` (default for `flick/js`; use `--runtime effect` for Effect) |
| `flick inspect <M> --emit js` | `flick inspect <M> --emit output --target js` |
| `flick inspect <M> --emit ts` | `flick inspect <M> --emit output --target js --typescript` |

### `flick.toml` keys

| Removed `[build]` key | Replacement |
| - | - |
| `[build] emit = "js"` | `[build] target = "js"` (or omit) |
| `[build] emit = "ts"` | `[build] target = "js"` and `[target.js] typescript = true` |
| `[build] emit = "go"` | `[build] target = "go"` |
| `[build] emit-dts = true` | `[target.js] declarations = true` |
| `[build] promise-facade = true` | `[target.js] promise-facade = true` |
| `[build] schema-aot = true` | `[target.js] schema-aot = true` |
| `[build] effect = "rt"` | `[target.js] runtime = "rt"` (or omit; `"rt"` is default) |
| `[build] effect = "effect"` | `[target.js] runtime = "effect"` |
| `[build] ui = "./renderer.mjs"` | `[target.js] ui = "./renderer.mjs"` |

Run `flick <cmd> --help` (or `flick <cmd> --target <name> --help`) to inspect
the options declared by the selected backend for that command. Unknown or invalid
options report `FLICK-P058`, and unknown target names report `FLICK-P059`.
`flick inspect <M> --emit iface` no longer loads any backend.

## Target dependencies move to `[target.<name>.dependencies]` and backends own package managers

Target-ecosystem package discovery (`package.json`, `node_modules`) and
package-manager commands (`install`, `add`, `remove`, `update`, `deps`, app
`pack`) now live in each backend component rather than in the core compiler:

- Rename `[js.dependencies]` in `flick.toml` to `[target.js.dependencies]`.
  The old `[js]` and `[js.dependencies]` tables are removed with no
  compatibility alias and fail with `FLICK-P057` (exit code `2`).
- Existing JavaScript projects that run `flick install`, `add`, `remove`,
  `update`, `deps`, or app `pack` with `flick/js` must grant `pnpm` in
  `flick.toml` (`FLICK-P051` without it; projects created with `flick new`
  include this automatically):

```toml
[tools]
"flick/js" = ["pnpm"]
```

- Go projects (`[build] target = "go"`) no longer read `package.json` or
  `node_modules` and need neither Node nor pnpm to build or test with Flick
  `[dependencies]`. Running `flick add <pkg>` without `--path`, `--git`, or
  `--url` under a backend without its own package manager reports `FLICK-P063`
  (exit code `2`).

## The IR carries full layout types (`0.14.0`)

The portable IR moves from `0.13.0` to `0.14.0` to carry the full layout-relevant
types inferred by the checker (`IrType`, `Module.types` with constructor field
types and GADT existentials/constraints, binder types, and `type_args` on
polymorphic globals, constructors and methods), deduplicated into a wire `types`
pool and shared behind `Arc` in memory. Effect, service and region rows remain
omitted.

Backend components and `ir`-hook plugins must rebuild against `flick-ir` `0.14.0`
and update their descriptor `ir` range to `[0.14.0, 0.15.0)`. IR validation
(`flick_ir::validate`, also enforced for `ir` plugins under `FLICK-P047`) now
checks type well-formedness, type argument counts and annotation agreement with
the terms they describe, while keeping erased nodes valid. Ordinary Flick
programs require no source changes; `flick/js` and `flick/go` ignore the new
compound type fields beyond bare type-constructor names.

## Backends declare foreign adapter metadata (`0.15.0`)

The portable IR moves from `0.14.0` to `0.15.0`: `flick_ir::Adapter` replaces
hardcoded `receiver`/`cancel`/`variadic` booleans with backend-declared adapter
words (`words: Vec<BackendAdapter>`, each carrying its name, typed arguments,
and source span) alongside core contract fields (`nullable`, `optional`,
`retention`, `owned`, `callbacks`). Each backend component now declares its own
foreign adapter words (`foreign-adapters`) and foreign type-form attributes
(`type-form-attributes`, such as `@ts` on `flick/js` or `@go` on `flick/go`) in
its WIT `descriptor` and validates its own adapter words in `check-target`
(including during `flick check` and editor analysis with `command = "check"`).

Backend components and `ir`-hook plugins must rebuild against `flick-ir`
`0.15.0` and `flick-backend` and update their descriptor `ir` range to
`[0.15.0, 0.16.0)`. Ordinary Flick bindings on `flick/js` and `flick/go`
require no source changes.

## Fixed-width numeric types in IR `0.16.0`

The portable IR moves from `0.15.0` to `0.16.0` to add the fixed-width numeric
built-ins (`Int8`, `Int16`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`,
`Float32`) as first-class `Lang`, `Num`, and `Lit` variants (`Int32` is an alias
for `Int`, and `Byte` is an alias for `UInt8`).

Backend components and `ir`-hook plugins must rebuild against `flick-ir` `0.16.0`
and update their descriptor `ir` range to `[0.16.0, 0.17.0)`. Ordinary Flick
programs require no source changes unless they previously defined top-level types
or modules named `Int8`, `Int16`, `Int32`, `Int64`, `UInt8`, `Byte`, `UInt16`,
`UInt32`, `UInt64`, or `Float32`.

## Inline foreign bindings and module preambles (`0.17.0`)

The portable IR moves from `0.16.0` to `0.17.0` to carry inline foreign code
with source spans: `flick_ir::Binding` is now an enum (`Module`, `Global`,
`Inline(InlineCode)`), `flick_ir::Module` gains `preambles: Vec<InlineCode>`
for `foreign module inline` declarations, and the WIT `build-request` gains
`inline-foreign: list<inline-foreign>`. Backends validate inline foreign code
and map syntax errors back to the `.fl` source span as `FLICK-F011`.

Backend components and `ir`-hook plugins must rebuild against `flick-ir`
`0.17.0` and `flick-backend` and update their descriptor `ir` range to
`[0.17.0, 0.18.0)`. Existing Flick sources require no changes.

## Versioned backend value ABI tables

The `flick:backend` WIT `descriptor` now includes an optional versioned
`value-abi` table keyed by lang item and structural type constructor (`tuple`,
`function`), recording host type, conversion (`none`, `copy`, `view`),
direction (`in`, `out`, `both`), opaque fallback type, and host `catch` error
type. `flick/js` publishes `flick.javascript-ffi@0.1.0` directly from backend
data, and English and Russian FFI and backend documentation render the official
table from that descriptor.

The type checker validates direct `foreign` declarations and selected `@native`
overrides against the active backend's `value-abi` table, reporting `FLICK-F013`
with the backend's refusal reason when a binding uses an unsupported type or
direction. Packages may declare `[package.abi]` in `flick.toml`, and
`BuildRequest.foreign` carries the active ABI identifier and version; version
mismatches report `FLICK-F014` rather than issuing a silent foreign call.
Existing JS FFI programs require no source changes; third-party backend
components should rebuild against the updated `flick-backend` WIT world.

## `FuncRef` and callback retention in IR `0.18.0`

`Prelude` declares `FuncRef (a -> b)`, a function without captures, and the
core module `FuncRef` gives `FuncRef.fun`. Where a `FuncRef` is expected, the
value must be a top-level function or a lambda that captures nothing;
anything else is `FLICK-F015` ([ffi.md](../../reference/ffi/#functions-without-captures-and-trampolines)).
A module that declares its own `FuncRef` type or module hides the built-in
one, as with any name of `Prelude`.

The portable IR moves from `0.17.0` to `0.18.0`: `flick_ir::Adapter` gains
`retention: Option<Retention>` (`call` or `owner`), so a backend that
generates callback trampolines from `callback i data j` knows how long to
keep their environment. `func_ref` is a value-ABI lang item without an
opaque fallback: a backend whose table has no `func_ref` row refuses a
`FuncRef` in a foreign signature with `FLICK-F013`; `flick/js` maps it to
the function itself. Backend components and `ir`-hook plugins must rebuild
against `flick-ir` `0.18.0` and update their descriptor `ir` range to
`[0.18.0, 0.19.0)`.

## Host exports and library build artifacts (`0.19.0`)

The portable IR moves from `0.18.0` to `0.19.0` to carry neutral `@export "name"`
host exports (`Module.exports.host`: `Vec<HostExport>`), and the `flick:backend`
WIT `build-request` gains `exports: list<host-export>` and
`headers: option<header-request>`. Backends validate exported host names,
package-wide uniqueness, and monomorphic non-dictionary exportability at the
`@export` span (`FLICK-P012`) and project library headers strictly (reporting
unexpressible exports as `FLICK-F004`). `flick/js` supports
`[target.js] kind = "library"` with `format = "esm" | "cjs" | "iife"`, optional
`global-name`, and `index.d.ts` emission.

Backend components and `ir`-hook plugins must rebuild against `flick-ir`
`0.19.0` and `flick-backend` and update their descriptor `ir` range to
`[0.19.0, 0.20.0)`. Existing Flick sources require no changes.

## Plugin protocol `0.5.0`

The `flick:plugin` component protocol moves from `0.4.0` to `0.5.0`:

- Plugins export `metadata() -> plugin-metadata` (`Plugin::metadata` in Rust,
  defaulting to empty metadata) declaring their block heads (`HeadSpec`),
  watched names (`watch`), and derived classes (`derives`). The parser reads
  both `std` and external plugin heads from a data-driven `HeadCatalog`,
  rejecting invalid specs, external `own_codes`, or head conflicts
  (`FLICK-P036`, `FLICK-P044`).
- `DeriveTarget`, `CaseTarget`, and `Written` carry deterministic declaration
  metadata: `name_span`, `param_spans`, `deriving_spans`, `visibility`,
  `attributes`, variant `docs`/`attributes`/`constraints`/`result`/`literal_value`,
  `RecordField` with `name_span`/`span`/`docs`/`source_index` (in canonical
  UTF-16 label order), and structured `WrittenKind::Forall`, `StringLit`, and
  `None`.
- WASM plugin components built for `flick:plugin@0.4.0` are rejected with
  `FLICK-P035` until rebuilt against `flick-plugin` (`0.5.0`). Existing Flick
  programs require no source changes.

## The kernel's defects

The primitive kernel `0.1.0` has two more primitives, `defect` and
`flickAssert` of `flick:rt` (see [the kernel](../../reference/kernel/#defects)).
`defect`, which the new core module `Flick.Defect` binds, stops pure code with
a message; `Int.unsafe_div`, `Int.unsafe_rem` and `todo` have Flick bodies over
it, so a backend without `@native` clauses of its own runs them through it.
`flickAssert`, a failed `let assert`, is a primitive now, as no Flick body can
render the value it missed.

A third-party backend implements both; a program that reaches one it lacks is
refused there. Flick sources need no change, unless a module of the project is
named `Flick.Defect`.

## `Math.log` in the kernel, and an exact `Float.next_pow2`

The kernel has the JS global `Math.log`, which the new `Float.log` binds:
the bits of fdlibm's `__ieee754_log`, which JS gives. Go's `math.Log`
differs from them in about one case in a thousand, so the Go runtime has a
port of fdlibm's function; a third-party backend implements it too.

`Float.next_pow2` is the least power of two at least `n`, as it says: on the
JS `rt` runtime and on Go it went through a ratio of logarithms, which made
`next_pow2 536870912.0` (2^29) `1073741824.0`. It is `536870912.0` now, as on
the `effect` runtime, and a negative `n` gives NaN everywhere.

## Hot-swap compatibility and declared state-cell migrations (formal model)

The λFlick reference model (`flick-model::hotswap`) and Lean mechanization
(`formal/Formal/HotSwap.lean`) now specify live-session hot swap and atomic
state-cell migrations:

- **Swap compatibility**: a replacement value must be closed and its scheme
  must subsume the previous scheme (`σ_new ≤ σ_old`). Generalizing a scheme or
  narrowing an effect row is accepted; narrowing a polymorphic scheme, widening
  an effect row, or narrowing a contravariant capture/region qualifier is
  rejected.
- **Two-version execution**: frames and closures already running keep their
  captured code and references to pre-migration store cells, while subsequent
  calls to the global cell enter the new code.
- **Atomic migrations**: pure `Old -> New` migration functions apply only to
  declared state cells. Committing extends the store typing with both versions;
  a failing or fuel-exhausted migration rolls back atomically with no state
  change.

Existing Flick sources require no changes. Differential `.fvm` image save/load
and migrations outside declared state cells are out of scope for this model
milestone; runtime VM hot swap builds on the exported compatibility checker and
session oracle.

## VM Tier 1 runs std and CLI intrinsics

The ownership IR (`flick-ir`) gains `Intrinsic::Cli(CliIntrinsic)` for audited
`flick:cli` builders (`command`, `description`, `flag`, `arg`, `subcommand`,
`handle`, `group`, `required`, `optional`, `many`, `run_version`), and both
in-process VM tiers (`--target vm --vm-tier 0` and `--vm-tier 1`) execute
schemas, codecs, JSON, temporal (`DateTime`, `Duration`, `TimeZone`), `Decimal`,
`BigInt`, `Arbitrary`, and `Cli` commands (argument parsing, typed `Cli.Value`
errors, help, version, and Bash/Zsh/Fish/PowerShell completions) through the
shared `flick-vm` standard library. JS and Go outputs are unchanged, and
existing Flick sources require no edits.
