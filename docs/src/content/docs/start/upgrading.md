---
title: Upgrading to 0.2
description: What changed since Flick 0.1.0, and how to update a project.
---

Flick 0.2 is in development on `main` and is not released yet. Before 1.0 a
minor release may break programs, and 0.2 does: `std` is a package the
project fetches, the `cli`, `codec` and `server fn` blocks and the derives of
`Schema`, `Url`, `Form`, `HttpApi` and `Cli.Value` moved from the compiler
into the plugin of `std`, and libraries are compiled from their sources.
This page lists each change since 0.1.0 and how to update a project. It
grows as more changes land.

## How to upgrade

Build the compiler from a checkout of `main` (see
[Ecosystem](../../ecosystem/)), point the project at the `std` of the same
checkout (see [the next section](#std-comes-from-a-package)), then run in
the project:

```sh
flick install
flick check --fix
```

`flick check --fix` applies the safe fixes: it rewrites `schema T` and adds
the imports that plugin blocks need, along with the other fixes it always
applies. It reports what is left, and the sections below say what each
change needs.

| Change | `flick check --fix` | What you see |
| - | - | - |
| [`std` comes from a package](#std-comes-from-a-package) | no: run `flick install` | `FLICK-P034` |
| [`schema T` is `T.schema`](#schema-t-is-tschema) | yes | `FLICK-T001` |
| [Plugin blocks need their import](#plugin-blocks-need-their-import) | yes | `FLICK-P037` |
| [Diagnostic codes under `std/`](#diagnostic-codes-under-std) | no | new codes |
| [`deriving HttpApi` is stricter](#deriving-httpapi-is-stricter) | no | `std/D011`, `FLICK-C001` |
| [Libraries are compiled from sources](#libraries-are-compiled-from-sources) | no | `FLICK-R003` |
| [Instances belong to one package](#instances-belong-to-one-package) | no | `FLICK-C006` |
| [Plugins reach only what is granted](#plugins-reach-only-what-is-granted) | no | `FLICK-P038`, `FLICK-P039` |
| [`LogLevel` is an enum](#loglevel-is-an-enum) | no | `FLICK-T001` |
| [Plugins follow the compiler's interface](#plugins-follow-the-compilers-interface) | no | — |
| [`CallSite` replaces the `_at` writes](#callsite-replaces-the-_at-writes) | no | `FLICK-T001` |
| [A closure over an error value says so](#a-closure-over-an-error-value-says-so) | no | `FLICK-G001` |
| [Rust 1.95 builds the compiler](#rust-195-builds-the-compiler) | no | — |

`flick check --fix page.md` rewrites the `flick` code blocks of a Markdown
page. There each block is checked on its own, with its imports taken on
trust, so a missing plugin import is not reported: add it by hand.

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

Until 0.2 is released, a compiler built from `main` still reports version
0.1.0, so it takes the `std` of the 0.1.0 tag, which lacks the functions
the compiler's plugins now write. Name the `std` of the same checkout
instead:

```toml
[dependencies]
std = { path = "../flick-lang/packages/std" }
```

or set `FLICK_STD_PATH` to that directory.

## `schema T` is `T.schema`

A type's witness is its companion `T.schema`, and `schema` is no keyword
any more. In 0.1:

```flick fail
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

## Plugin blocks need their import

`cli`, `codec`, `server fn`, `sql table` and `sql migration` blocks are
plugins of `std` now. A file that writes one imports its head:

| Block | Import |
| - | - |
| `cli "name" { … }` | `import Cli exposing (plugin cli)` |
| `codec s { … }` | `import Codec exposing (plugin codec)` |
| `server fn f x = …`, `server f` | `import Server exposing (plugin server)` |
| `sql table Name "table" { … }`, `sql migration 1 "name" { … }` | `import Sql exposing (plugin sql)` |

In 0.1 a `codec` block needed no import:

```flick ignore
type alias Profile = { email : String; nickname : Option String }
  deriving Schema

profile_json : PureCodec Json Profile
let profile_json =
  codec (schema Profile) {
    email = field "email_address" String.json;
    nickname = field "nickname" (option_as_null String.json);
  }
```

Now the block without its import is `FLICK-P037`, whose fix adds the
import:

```flick
import Codec exposing (plugin codec)

type alias Profile = { email : String; nickname : Option String }
  deriving Schema

profile_json : PureCodec Json Profile
let profile_json =
  codec Profile.schema {
    email = field "email_address" String.json;
    nickname = field "nickname" (option_as_null String.json);
  }
```

`flick check --fix` adds the import and rewrites `schema Profile` in the
same run. `cli`, `codec`, `server` and `fn` are no keywords any more;
`plugin` is a keyword only in the exposed items of an import.

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
| `LogLevel.none_` | `LogLevel.Off`, whose literal is Effect's `"None"` |
| `LogLevel.values` | `LogLevel.all`, in order of severity |
| `LogLevel.name level` | `LogLevel.value level` |
| `LogLevel.get_ordinal a < LogLevel.get_ordinal b` | `compare a b`, the derived `Ord` |

`show` of a level gives its constructor, `Off` where 0.1.0 printed `None`.
Each old name is `FLICK-T001`; replace it by hand.

## Plugins follow the compiler's interface

A derive of an enum gets the literals of its constructors, and a plugin's
`check` the base type of an enum; a derive knows which constructors are of
a GADT (`refined`, `constrained` of a case), and a plugin asks while it
expands whether a type is an enum (`literal-of`). The interface stays
`0.1.0` until Flick 0.2 and changes without a new version: rebuild a
component with the `flick-plugin` of the compiler it runs in.

## `CallSite` replaces the `_at` writes

The core class `CallSite` gives a function the place of its use (see
[Call sites](../../reference/classes/#call-sites)). `Atom.set`, `update` and
`modify` take it themselves, so `Atom.set_at`, `update_at` and `modify_at`
are gone (`FLICK-T001`): call `Atom.set atom value`, or give your own
function `CallSite =>` to report its caller's place. A serializable atom's
key is still `Module.name`, now its `CallSite.definition`.

## A closure over an error value says so

A value of `Errors e`, `Exit e a` or `Failure e` holds what the errors of
`e` hold, and an error-row variable `e` is held as a type variable is. In
0.1.0 such a value held nothing, so a closure over a caught error could
hide a region its payload has. A curried stage keeps the variable, so
`keep : forall e. Errors e -> (Unit -> Unit)` still checks and reads as
holding `e`. A bare type that is no stage, such as a field, now rejects the
closure:

```flick fail
keep_field : forall e. Errors e -> { run : Unit -> Unit }
let keep_field x = { run = fun () -> ignore x }
```

```text
error[FLICK-G001]: this function uses `x`, which holds `e`, but the expected type `Unit -> Unit` holds no captures
```

Name what it holds with a capture row, as for a type variable (see
[Captures](../../reference/regions/#captures)):

```flick
keep_field : forall c e. (Errors e)^c -> { run : (Unit -> Unit)^c }
let keep_field x = { run = fun () -> ignore x }
```

`flick check --fix` does not rewrite it: the capture row goes into the
`forall`, the parameter and the field at once.

## Rust 1.95 builds the compiler

A checkout of `main` builds with Rust 1.95 (0.1.0 took 1.94); its
`rust-toolchain.toml` names the version for rustup. The `server-stdlib`
Cargo feature is gone: every build has the plugin of `std`.

## Planned for 0.2

These changes are planned and not done yet; each gets a section here when it
lands:

- `sql` blocks as a plugin of `std`, as `cli` and `codec` are now;
- later, changes to the core of effects.
