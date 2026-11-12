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

or set `FLICK_STD_PATH` to that directory.

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
assigns (see [Variables](../../reference/types/#variables)). The word `mut`
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
parameter any row, in the result what the parameters before it reach (see
[Regions and captures](../../reference/regions/#qualifiers)).

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
