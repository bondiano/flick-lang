---
title: Why Flick
description: What Flick is for, who it suits, and who it does not suit yet.
---

Flick is a functional language for applications that run in a browser, on a
server and on the command line. It compiles to JavaScript for the browser
and Node, and to Go for a standalone executable. This page says what that
gives you, and what it does not give you yet.

## One language from the browser to the command line

The same modules build for two targets:

- **JavaScript.** ES modules for the browser and Node. `ui` views render
  with React, and effects run on [Effect](https://effect.website/) through
  `@flick-lang/runtime`.
- **Go.** `flick build --emit go` builds an app into one executable that
  needs no Node.

A library package is compiled for the target of each package that depends
on it. Pure code, such as types, validation, routes and codecs, is written
once and shared. In the [Conduit example](https://github.com/bondiano/flick-lang/tree/main/examples/conduit),
the API contract is one package. The Go server and the browser client both
depend on it, so a mismatch between them is a type error.

## Effectful code reads like ordinary code

You do not need to know what a monad is to write Flick. In a runtime
function, a service call is a direct call, `let` binds its result, and the
lines run from top to bottom. This code is from the
[first slice example](https://github.com/bondiano/flick-lang/tree/main/examples/first-slice):

```flick
# opaque type alias UserId = String
# type alias User = { id : UserId; name : String }
# type alias UserView = { id : UserId; title : String }
# to_view : User -> UserView
# let to_view user = { id = user.id; title = String.trim user.name }
#
type error NotFound = NotFound UserId

service Users =
  get : UserId ->{; NotFound} User

service Log =
  info : String ->{} Unit

load_view : UserId ->{Users, Log; NotFound} UserView
let load_view id =
  let user = Users.get id
  let view = to_view user
  Log.info ("Loaded " ++ view.title)
  view

load_two : UserId -> UserId ->{Users, Log; NotFound} (UserView, UserView)
let load_two first_id second_id =
  let first = task { load_view first_id }
  let second = task { load_view second_id }
  parallel first second
```

`load_view` has no `yield`, `flatMap` or `pipe`: the compiler sequences the
calls. `task { … }` makes a call a value without running it, and `parallel`
runs both concurrently and returns both results. Local mutable variables
and general loops are planned; they are not in the language yet.

## Safety from the types, not from discipline

Three things a reviewer would otherwise check by hand are in the types:

- **Effects.** `a -> b` is a pure function. `a ->{Users, Log; NotFound} b`
  is a runtime call that needs the services `Users` and `Log`. A runtime call
  in a pure function is a compile error (`FLICK-E001`), and so is a body that
  needs a service or an error its signature does not name (`FLICK-E002`).
- **Errors.** `NotFound` is a `type error`, part of the error row.
  `try … with` removes it only when the handlers cover every value of it;
  an error left unhandled stays in the caller's signature.
- **Resources.** `scope { … }` releases what it acquired when it ends. The
  region checker proves that nothing usable outlives the scope, including a
  resource hidden in a closure or a `task`.

The closure below would read a file after its scope has closed it. The
compiler rejects it with `FLICK-G001`: the closure holds the scope's region,
and its type allows none.

```flick fail
# import File exposing (Path, FileSystem, OpenError, ReadError)
#
read_later : Path ->{FileSystem; OpenError} (Unit ->{FileSystem; ReadError} String)
let read_later path =
  scope {
    let file = File.open_scoped path
    fun () -> File.read_all file
  }
```

[Effects, errors and services](../../reference/effects/) and
[Regions and captures](../../reference/regions/) have the full rules.

## Use the host's libraries instead of rewriting them

Flick does not need its own copy of every library. A `foreign` declaration
binds a JavaScript function to a Flick signature, and the tools write and
check most of these declarations:

- `flick ffi import <package> --out <dir>` reads the `.d.ts` of an installed
  npm package and writes Flick bindings, with a report on every export:
  supported, needs a policy, or unsupported, with the reason. The import
  infers nothing a declaration cannot show: a call is never assumed pure,
  and a Promise needs an error conversion.
- In an editor, the code action "Regenerate from `<pkg>@<version>`" on the
  header line of a generated module writes it again from the installed
  declarations. `flick ffi check` reports bindings whose declarations have
  changed.
- In the other direction, `flick build --emit-dts` writes TypeScript
  declarations of Flick modules, so TypeScript calls Flick with its types.
  `--promise-facade` adds a Promise entry point to a runtime function.

For `lookup(id: string): Promise<string | undefined>`, with a policy that
says how it fails and what `undefined` means, the import gives this
binding:

```flick
# import JsError exposing (JsError)
#
lookup : String ->{; JsError} Option String
foreign async lookup from "lookup-lib" "lookup"
  arity 1
  catch JsError.from_js
  nullable
```

On the Go target, a binding to a module of the program calls the Go
function of the same name in a `.go` file beside it. See
[Foreign functions](../../reference/ffi/) and the TypeScript section of
[Code generation](../../reference/codegen/#typescript).

## Code a model writes and a person reviews

Flick suits code that a language model writes and a person reviews:

- **Signatures are required.** Every named function has its full signature
  right above its `let`. A function without one is `FLICK-T004`, and the
  message suggests the inferred type. The signature says what the function
  needs, how it can fail and whether it is pure, and the compiler holds the
  body to it.
- **Mistakes are compile errors with codes.** Misplaced effects, unhandled
  errors, escaping resources and incomplete matches are reported before the
  program runs. `flick explain <code>` explains a code, and
  `--diagnostics json` gives tools one event per line.
- **The standard library covers a typical application.** HTTP APIs, RPC,
  typed PostgreSQL queries, schemas and codecs, command lines, React views,
  tracing and metrics, cryptography and JWTs are part of it. There are
  fewer packages to choose and to learn. See the
  [overview](../../reference/stdlib/).

The source is about as long as the same app in Go and React. `just tokens`
counts the hand-written source of `examples/conduit` (the contract, the Go
server and the browser client) against the RealWorld reference of the same
app: a Go (gin) backend and a React/Redux client, each pinned to a commit.
Tests, vendored files and generated files are not counted.

| Tokenizer | Flick | Reference | Ratio |
| - | -: | -: | -: |
| Claude (Opus 5.5) | 48,180 | 52,034 | 0.93 |
| o200k_base (GPT) | 30,018 | 29,778 | 1.01 |

By Claude's tokenizer, Flick is about 7% shorter (0.98 for the server, 0.87
for the client). By GPT's, the two are about even. The method is in
[`tools/tokens`](https://github.com/bondiano/flick-lang/blob/main/tools/tokens/index.mjs).

## Three examples

### A full-stack web app

[`examples/conduit`](https://github.com/bondiano/flick-lang/tree/main/examples/conduit)
is RealWorld's Conduit, a Medium clone: a Go server over PostgreSQL and a
single-page client rendered with React, which share one typed contract of
the API. The endpoints are records that derive `HttpApi`, so a misspelt
path is a type error, and the OpenAPI document comes from the same value.
The queries are `sql` blocks checked against the migrations. The example
needs no foreign bindings, and the official RealWorld API and frontend
suites run against it.

### A command-line tool on two targets

[`examples/cli-tool`](https://github.com/bondiano/flick-lang/tree/main/examples/cli-tool)
is a notes CLI described by one `cli` block: subcommands, a wizard that
asks for the first note, help, version and shell completions. The logic is
a pure module, so its tests need no file. The same source builds as a Go
executable without Node, and prints the same help, errors and completions.

### Services you replace in tests

[`examples/first-slice`](https://github.com/bondiano/flick-lang/tree/main/examples/first-slice)
is where the code above comes from. `App.Main` provides the real `Users`
and `Log` as layers, in one place. The tests provide implementations over a
fixed list and a `Ref` instead, without changing the code under test.

The [examples](https://github.com/bondiano/flick-lang/tree/main/examples)
also include a shop that reports to an OpenTelemetry collector and a
planner built on monad transformers written in Flick.

## Who Flick is not for (yet)

- **Projects that need a stable language.** Flick is at version 0.1, and
  the language is still changing. Before 1.0, a minor release may break
  programs.
- **Teams that expect a large ecosystem.** Beyond the standard library,
  there are few Flick packages. A JavaScript library is used through
  bindings you generate and review.
- **Pages that must ship little JavaScript.** A UI program's bundle carries
  the Effect runtime and React: the interactive UI programs this repository
  tracks are held to budgets of 117–126 KB gzipped. A program can be built
  without Effect (`effect = "none"`), but then it makes no runtime calls
  and uses only a few core modules, such as `List` and `String`. Moving
  Effect out of the language core is planned.
- **Other platforms.** There is no mobile, desktop or WebAssembly target;
  these are directions, not features. On Go, parts of the standard library
  and some `foreign` metadata do not compile yet; the compiler reports them
  when it builds, never at run time.
- **Teams already productive with TypeScript and Effect.** On JavaScript,
  Flick runs on the same Effect, so adopting it is a choice of source
  language, not of runtime. [Flick and Effect](../../learn/flick-and-effect/)
  compares the two.

## Next

[Getting started](../getting-started/) installs the compiler and runs a
first app. [Learn Flick](../../learn/) introduces the ideas in order.
