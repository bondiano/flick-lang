---
title: Designing a program with effects
description: Separate domain logic from external calls, provide services at the entry point, and replace them in tests.
---

A user lookup is small enough to write in one function: call a JavaScript
directory, trim the name, log the result. As soon as you want to test it,
you need control over that directory and the log. Flick's services let you
name those dependencies in the program's signature and choose their
implementations at its entry point.

This chapter follows
[`examples/first-slice`](https://github.com/bondiano/flick-lang/tree/main/examples/first-slice).
It loads two users, formats their names and handles a missing user. You will
see how to keep the transformation pure, make failures explicit, connect
external implementations and test the same logic with local data.

You should already be able to [run a Flick project](../../start/getting-started/)
and read a function and a `match`. The code below is a sequence of excerpts;
the linked example contains the complete modules to run together.

## Run the example

Use a checkout of the same revision for the compiler and the example; this
chapter follows `main`. With the repository's
[development dependencies installed](https://github.com/bondiano/flick-lang/blob/main/CONTRIBUTING.md#setup),
run these commands from its root:

```sh
cargo build -p flick
./target/debug/flick --project examples/first-slice check
./target/debug/flick --project examples/first-slice run
./target/debug/flick --project examples/first-slice test
```

A run can print:

```text
[log] Loaded Grace Hopper
[log] Loaded Ada Lovelace
Ada Lovelace and Grace Hopper
not found: "u3"
```

The directory deliberately answers the second lookup sooner. The two log
lines may appear in either order; the result still says Ada before Grace.
The test command runs three tests: name trimming, two successful loads, and
a missing user that produces no success log.

There are four Flick modules and a small JavaScript directory:

| File | Responsibility |
| - | - |
| `src/App/Users.fl` | Domain types, service contracts and lookup logic |
| `src/App/Live.fl` | Implementations over the JavaScript directory and console |
| `src/App/directory.mjs` | A simulated external directory that answers after a delay |
| `src/App/Main.fl` | Choose implementations, handle expected failures and print results |
| `test/App/UsersTest.fl` | Local implementations and assertions |

## Keep the transformation pure

Start with the part that needs only data. In `App.Users`, a user's name is
trimmed to make a display title:

```flick
opaque type alias UserId = String
  deriving Eq, Show

type alias User = {
  id : UserId;
  name : String;
}

type alias UserView = {
  id : UserId;
  title : String;
}

/// The only way to make a `UserId` outside this module.
user_id : String -> UserId
let user_id raw = raw

to_view : User -> UserView
let to_view user = {
  id = user.id;
  title = String.trim user.name;
}
```

`to_view` has a plain arrow, `User -> UserView`. It can be tested with an
ordinary value. It does not decide where a user comes from or where to
write a log.

`UserId` is opaque outside `App.Users`: callers see a distinct type whose
internal representation, a string in this example, is hidden. They can store
and pass an identifier, but cannot substitute an ordinary string for `UserId`
or apply string operations to it. Callers use `user_id` to construct one.
Here that function wraps a string without validating it; an
opaque type alone does not make an identifier valid.

## See what direct external calls couple together

Before introducing services, a lookup could call the foreign functions
directly. This starting version is illustrative; the completed example
places these bindings in `App.Live`:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# /// The only way to make a `UserId` outside this module.
# user_id : String -> UserId
# let user_id raw = raw
#
# to_view : User -> UserView
# let to_view user = {
#   id = user.id;
#   title = String.trim user.name;
# }
#
type error NotFound = NotFound UserId
  deriving Show

find_name : UserId ->{Async} Option String
foreign find_name from "./directory.mjs" "findName"
  nullable

console_log : String ->{} Unit
foreign console_log from global "console.log"

load_view_direct : UserId ->{Async; NotFound} UserView
let load_view_direct id =
  let user =
    match find_name id with
    | Some name -> { id = id; name = name }
    | None -> from_result (Err (NotFound id))
  let view = to_view user
  console_log "[log] Loaded \{view.title}"
  view
```

`nullable` translates a JavaScript `null` or `undefined` to `None`. An absent
user becomes the typed error `NotFound`; an existing user is transformed
and logged.

`from_result` returns the value of an `Ok` or raises the typed error carried
by an `Err`. Here it turns the missing-user branch into the failure named
in the function's signature.

This function fixes both choices: which directory to call and where the
log goes. A test of a successful lookup has to deal with that directory's
delay and the console. Passing a different identifier does not replace
either dependency.

The runtime arrow `->{Async; NotFound}` says the call may suspend and fail
with `NotFound`. It does **not** describe every external capability: foreign
calls and host operations such as console output are not all named in
Flick's effect rows yet. An empty service row is not a sandbox. See
[where the promise stops](../../start/why/#where-the-promise-stops).

## Name the operations the program needs

Move the contracts into `App.Users`, alongside the domain types:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
type error NotFound = NotFound UserId
  deriving Show

service Users =
  get : UserId ->{Async; NotFound} User

service Log =
  info : String ->{} Unit
```

`Users` promises one operation. Calling `Users.get` needs a `Users`
implementation, may suspend (`Async`) and may fail with `NotFound`. `Log`
promises an `info` operation. Its declared empty row means the operation
requires no further services or typed errors; calling it still requires
`Log` itself.

Keep operations close to the domain's needs. The lookup asks for a user by
identifier, so the contract contains `get`. It does not expose a JavaScript
module, a connection pool or an HTTP client to every caller.

The program now calls these operations:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
# /// The only way to make a `UserId` outside this module.
# user_id : String -> UserId
# let user_id raw = raw
#
# to_view : User -> UserView
# let to_view user = {
#   id = user.id;
#   title = String.trim user.name;
# }
#
load_view : UserId ->{Async, Users, Log; NotFound} UserView
let load_view id =
  let user = Users.get id
  let view = to_view user

  Log.info "Loaded \{view.title}"

  view
```

Read `UserId ->{Async, Users, Log; NotFound} UserView` from left to right:

| Part | Meaning |
| - | - |
| `UserId` | The input |
| `Async` | The computation may suspend |
| `Users, Log` | The caller must provide these services |
| `; NotFound` | A typed failure the caller must handle or propagate |
| `UserView` | The successful result |

The body reads like ordinary sequential code. If `Users.get` fails,
`to_view` and `Log.info` are not reached. There is no success log for a
missing user.

A wrapper must account for the operations it uses too. This version omits
`Log` from its signature and is rejected with `FLICK-E002`:

```flick fail E002
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
load_name : UserId ->{Async, Users; NotFound} String
let load_name id =
  let user = Users.get id
  Log.info "Loaded \{user.name}"
  user.name
```

Add `Log` to that row, or provide its implementation around the call. A
signature can allow more than a body needs, but the body cannot silently
add a service or typed error. The
[effects reference](../../reference/effects/) gives the full rules.

## Put external implementations at the boundary

`App.Live` owns the foreign bindings and uses them to build the two layers:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
find_name : UserId ->{Async} Option String
foreign find_name from "./directory.mjs" "findName"
  nullable

console_log : String ->{} Unit
foreign console_log from global "console.log"

users_live : Layer none none {Users}
let users_live =
  Layer.succeed Users {
    get = fun id ->
      match find_name id with
      | Some name -> { id = id; name = name }
      | None -> from_result (Err (NotFound id));
  }

log_live : Layer none none {Log}
let log_live = Layer.succeed Log { info = fun line -> console_log "[log] \{line}" }
```

`Layer.succeed Users` takes a record with the operations of `Users`.
`users_live` keeps the conversion from a nullable external result to the
domain's `NotFound` in one place. `log_live` decides how an application log
reaches the console.

`Layer none none {Users}` describes **building the layer**: it needs no
services, has no typed construction error and supplies `Users`. It does not
say that `Users.get` is pure or cannot fail. The operation retains its
`Async` and `NotFound` contract when it is called.

`App.Users` can now stay independent of the directory binding and console.
Another implementation could use an HTTP endpoint while keeping the same
operation contract; a different error model would require an explicit
change to that contract.

## Compose work without choosing implementations

The example also loads two users in parallel, still inside `App.Users`:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
# /// The only way to make a `UserId` outside this module.
# user_id : String -> UserId
# let user_id raw = raw
#
# to_view : User -> UserView
# let to_view user = {
#   id = user.id;
#   title = String.trim user.name;
# }
#
# load_view : UserId ->{Async, Users, Log; NotFound} UserView
# let load_view id =
#   let user = Users.get id
#   let view = to_view user
#
#   Log.info "Loaded \{view.title}"
#
#   view
#
load_two : UserId -> UserId ->{Async, Users, Log; NotFound} (UserView, UserView)
let load_two first_id second_id =
  let first = task { load_view first_id }
  let second = task { load_view second_id }

  parallel first second
```

`task { ... }` makes work to run. `parallel` runs the two tasks and returns
results in their argument order, even when the second finishes first.
Neither task chooses a directory or a logger: both use the services
provided around the call.

Parallel work does not make side effects ordered. That is why the example
can print Grace's log first while returning Ada's view first. Its test
sorts the captured logs and checks result order separately.

If one task fails, `parallel` interrupts and awaits the other before it
returns the failure. Effects already performed, such as a log line, are
not rolled back. See the [concurrency contract](../../reference/runtime/#concurrency).

## Assemble the application in `main`

`App.Main` imports the layers from `App.Live` and the operations from
`App.Users`. It handles the expected failure where it can choose a
user-facing response:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
# /// The only way to make a `UserId` outside this module.
# user_id : String -> UserId
# let user_id raw = raw
#
# to_view : User -> UserView
# let to_view user = {
#   id = user.id;
#   title = String.trim user.name;
# }
#
# load_view : UserId ->{Async, Users, Log; NotFound} UserView
# let load_view id =
#   let user = Users.get id
#   let view = to_view user
#
#   Log.info "Loaded \{view.title}"
#
#   view
#
# load_two : UserId -> UserId ->{Async, Users, Log; NotFound} (UserView, UserView)
# let load_two first_id second_id =
#   let first = task { load_view first_id }
#   let second = task { load_view second_id }
#
#   parallel first second
#
# find_name : UserId ->{Async} Option String
# foreign find_name from "./directory.mjs" "findName"
#   nullable
#
# console_log : String ->{} Unit
# foreign console_log from global "console.log"
#
# users_live : Layer none none {Users}
# let users_live =
#   Layer.succeed Users {
#     get = fun id ->
#       match find_name id with
#       | Some name -> { id = id; name = name }
#       | None -> from_result (Err (NotFound id));
#   }
#
# log_live : Layer none none {Log}
# let log_live = Layer.succeed Log { info = fun line -> console_log "[log] \{line}" }
#
describe : String ->{Async, Users, Log} String
let describe raw =
  try (load_view (user_id raw)).title with
  | NotFound id -> "not found: \{show id}"

program : Unit ->{Async, Users, Log} Unit
let program () =
  let both =
    try
      let (first, second) = load_two (user_id "u1") (user_id "u2")
      "\{first.title} and \{second.title}"
    with
    | NotFound id -> "not found: \{show id}"
  Platform.stdout_line both
  Platform.stdout_line (describe "u3")

main : Unit ->{Async; AppError} Unit
let main () = provide Layer.merge users_live log_live { program () }
```

`describe` catches `NotFound`, so its signature no longer lists that error.
It still needs `Users` and `Log`, and it may suspend. The same decision in
`program` gives a message when either parallel lookup finds no user.
Catching an expected lookup error here lets the domain code keep it typed.

`Layer.merge users_live log_live` supplies both services.
`provide ... { program () }` makes them available for the body, so `main`
no longer requires `Users` or `Log`. `Async` remains: supplying services
does not remove suspension. `AppError` is allowed by the entry signature;
this body does not need to raise one.

This is the place to choose the live implementations for this application.
The two `Platform.stdout_line` calls also belong here: they decide how to
present the program's results.

## Test by providing different services

The test module imports `App.Users` and supplies local implementations. Its
users come from a fixed list, and its log writes to a `Ref`:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
# /// The only way to make a `UserId` outside this module.
# user_id : String -> UserId
# let user_id raw = raw
#
# to_view : User -> UserView
# let to_view user = {
#   id = user.id;
#   title = String.trim user.name;
# }
#
ada : User
let ada = { id = user_id "u1"; name = "  Ada  " }

grace : User
let grace = { id = user_id "u2"; name = "Grace" }

users_test : Layer none none {Users}
let users_test =
  Layer.succeed Users {
    get = fun id ->
      match List.find (fun user -> eq user.id id) [ada, grace] with
      | Some user -> user
      | None -> from_result (Err (NotFound id));
  }

log_test : Ref (List String) -> Layer none none {Log}
let log_test lines =
  Layer.succeed Log { info = fun line -> Ref.update lines (fun xs -> List.append xs [line]) }
```

`users_test` implements the same contract without waiting for the external
directory. An implementation may need fewer effects than its contract
allows. `log_test` receives a fresh log buffer for each effect test, so the
tests do not share recorded lines.

The example then exports these three tests:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
# /// The only way to make a `UserId` outside this module.
# user_id : String -> UserId
# let user_id raw = raw
#
# to_view : User -> UserView
# let to_view user = {
#   id = user.id;
#   title = String.trim user.name;
# }
#
# load_view : UserId ->{Async, Users, Log; NotFound} UserView
# let load_view id =
#   let user = Users.get id
#   let view = to_view user
#
#   Log.info "Loaded \{view.title}"
#
#   view
#
# load_two : UserId -> UserId ->{Async, Users, Log; NotFound} (UserView, UserView)
# let load_two first_id second_id =
#   let first = task { load_view first_id }
#   let second = task { load_view second_id }
#
#   parallel first second
#
# ada : User
# let ada = { id = user_id "u1"; name = "  Ada  " }
#
# grace : User
# let grace = { id = user_id "u2"; name = "Grace" }
#
# users_test : Layer none none {Users}
# let users_test =
#   Layer.succeed Users {
#     get = fun id ->
#       match List.find (fun user -> eq user.id id) [ada, grace] with
#       | Some user -> user
#       | None -> from_result (Err (NotFound id));
#   }
#
# log_test : Ref (List String) -> Layer none none {Log}
# let log_test lines =
#   Layer.succeed Log { info = fun line -> Ref.update lines (fun xs -> List.append xs [line]) }
#
tests : List Test.Test
let tests = [
  Test.test "to_view trims the name" (fun () -> Test.equal "Ada" (to_view ada).title),
  Test.effect_test
    "load_two loads both and logs each"
    (task {
      let lines = Ref.make []
      let (first, second) = provide Layer.merge users_test (log_test lines) {
        load_two (user_id "u1") (user_id "u2")
      }
      Test.expect (Test.equal ["Ada", "Grace"] [first.title, second.title])
      Test.expect (Test.equal ["Loaded Ada", "Loaded Grace"] (List.sort (Ref.get lines)))
    }),
  Test.effect_test
    "load_view fails with NotFound"
    (task {
      let lines = Ref.make []
      let outcome =
        try
          let view = provide Layer.merge users_test (log_test lines) {
            load_view (user_id "nobody")
          }
          view.title
        with
        | NotFound id -> "missing \{show id}"
      Test.expect (Test.equal "missing \"nobody\"" outcome)
      Test.expect (Test.equal [] (Ref.get lines))
    }),
]
```

The first test checks a pure transformation directly. The second runs the
real `load_two` with test services and checks both the returned views and
the logs. The third checks the failure path and verifies that it wrote no
success log. None of them needs to replace a global foreign function or
capture process output.

Notice that the log assertion for parallel work compares sorted lines.
Asserting an incidental completion order would make the test depend on the
scheduler rather than the lookup's contract.

## Apply the same design to your program

When adding a feature, start with the values it transforms and keep that
transformation pure where possible. For each external dependency, name the
operations the feature needs in a service. Let the signature carry those
services, suspension and expected failures until a caller can supply an
implementation or make a recovery decision.

Choose live layers at the application boundary and provide local layers
in tests. When a layer acquires a resource such as a connection, give it a
scoped lifetime; `Layer.succeed` is enough here because these layers acquire
no resource. Continue with [layers and provide](../../reference/effects/#layers-and-provide),
[regions and captures](../../reference/regions/), or the
[foreign interface](../../reference/ffi/) for those details.
