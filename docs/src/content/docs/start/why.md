---
title: Why Flick
description: What Flick promises, where the promise stops, and who it does not suit yet.
---

Flick is a language for whole apps: the browser, the server and the command
line. Its main idea fits in one signature:

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
```

Read `load_view` from left to right. It takes a `UserId`, needs the services
`Users` and `Log`, can fail with `NotFound`, and returns a `UserView`. The
compiler holds the body to that. If the body also called another service, this
would not compile.

## What the compiler holds you to

- **Pure code is pure.** `a -> b` is a pure function. A runtime call inside one
  is `FLICK-E001`.
- **Services and errors are named.** A body that needs a service or an error its
  signature does not name is `FLICK-E002`. The compiler offers to add it.
- **Errors get handled.** `try … with` removes `NotFound` only when its handlers
  cover every value of it. Otherwise the error stays in the caller's signature.
- **Every top-level function has a signature.** One without it is
  `FLICK-T004`, and the message suggests the inferred type. Only a local function
  may leave it out, since nothing outside its body sees it.
- **Every `match` is complete.** A value it misses is `FLICK-T009`.
- **Resources stay inside their scope.** `scope { … }` releases what it
  acquired when it ends, and nothing usable can outlive it.

The closure below would read a file after its scope has closed it. The compiler
rejects it with `FLICK-G001`.

```flick fail
# import File exposing (Path, FileSystem, OpenError, ReadError)
#
read_later : Path ->{Async, FileSystem; OpenError} (Unit ->{Async, FileSystem; ReadError} String)
let read_later path =
  scope {
    let file = File.open_scoped path
    fun () -> File.read_all file
  }
```

[Effects, errors and services](../../reference/effects/) and
[Regions and captures](../../reference/regions/) have the full rules.

## Where the promise stops

- **Host calls are not named yet.** `Console.log`, `Platform.env`,
  `DateTime.now` and `Random.next_int` are `->{}` functions. Such a signature
  says only that the function runs in the runtime, not what it touches.
- **Defects stay outside the signature.** A pure function can still die with a
  defect or never finish. A runtime function can also be interrupted.
- **`foreign` is taken on trust.** The compiler believes the signature of a
  foreign binding. `flick ffi check` compares it only with the package's
  `.d.ts`.
- **The proofs cover the core, not the whole compiler.** The core calculus is
  proved in Lean under stated hypotheses, and the known differences from the
  checker are written down. See [`formal/`](https://github.com/bondiano/flick-lang/tree/main/formal).

## Effects read like ordinary code

You do not need to know what a monad is to write Flick. A service call is a
direct call, `let` binds its result, and the lines run from top to bottom.
`load_view` above has no `yield`, `flatMap` or `pipe`. To run work concurrently,
make the calls values with `task`:

```flick
# opaque type alias UserId = String
# type alias UserView = { id : UserId; title : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} UserView
# service Log =
#   info : String ->{} Unit
# load_view : UserId ->{Users, Log; NotFound} UserView
# let load_view id = Users.get id
#
load_two : UserId -> UserId ->{Users, Log; NotFound} (UserView, UserView)
let load_two first_id second_id =
  let first = task { load_view first_id }
  let second = task { load_view second_id }
  parallel first second
```

Local mutable variables and general loops are planned; they are not in the
language yet.

## One language for the whole app

The same modules build for two targets. JavaScript gives ES modules for the
browser and Node: `ui` views render with React, and effects run on
[Effect](https://effect.website/) for now. Go gives one executable that needs no
Node. `just language` compares the behaviour of both targets in CI.

Pure code is written once and shared. In the
[Conduit example](https://github.com/bondiano/flick-lang/tree/main/examples/conduit),
a Go server over PostgreSQL and a browser client depend on one package with the
API contract. A route that does not exist is a compile error on both sides, and
`sql` blocks are checked against the migrations, not against a live database.

There are limits. `ui` runs only on JavaScript, and parts of the standard
library do not compile to Go yet. The compiler reports that as `FLICK-P012` when
it builds, never at run time.

## Use the host's libraries

Flick does not need its own copy of every library. A `foreign` declaration binds
a JavaScript function to a Flick signature, and `flick ffi import <package>`
writes most of them from the package's `.d.ts`, with a report on every export.
In the other direction, `flick build --emit-dts` writes TypeScript declarations
of Flick modules. On Go, bindings are written by hand for now. See
[Foreign functions](../../reference/ffi/).

## Code you did not write

A teammate, a library or a model wrote it, and you are reviewing it. The
signature tells you what it needs and how it fails before you read the body.
Mistakes are compile errors with codes, `flick explain <code>` explains a code,
and `--diagnostics json` gives tools one event per line.

The source is about as long as the same app in Go and React.
[`tools/tokens`](https://github.com/bondiano/flick-lang/blob/main/tools/tokens/index.mjs)
counts `examples/conduit` against the RealWorld reference of the same app. With
GPT's `o200k_base` the two are about even: 30,174 tokens against 29,778, a ratio
of 1.01. With Claude's tokenizer the ratio was 0.93 when last measured. It is
one app, so treat these as a sanity check.

## Who Flick is not for yet

- **Projects that need a stable language.** Flick is at version 0.1, and a minor
  release may break programs before 1.0.
- **Teams that expect a large ecosystem.** Beyond the standard library there are
  few Flick packages.
- **Pages that must ship little JavaScript.** A UI program carries the Effect
  runtime and React: the UI programs this repository tracks are held to budgets
  of 117–126 KB gzipped.
- **Other platforms.** There is no mobile, desktop or WebAssembly target.
- **Teams already productive with TypeScript and Effect.** On JavaScript, Flick
  runs on the same Effect, so adopting it is a choice of source language, not of
  runtime. [Flick and Effect](../../learn/flick-and-effect/) compares the two.

## Next

[Getting started](../getting-started/) installs the compiler and runs a first
program. [Learn Flick](../../learn/) introduces the ideas in order.
