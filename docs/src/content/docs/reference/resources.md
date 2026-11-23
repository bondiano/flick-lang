---
title: "Resources and disposal"
---

A `scope` owns what is acquired into it and releases it when its body ends
([regions](../regions/)). A value of a class `Close` is released through
`Resource.own`: the scope closes it once, with how its body ended, and no
other code can close it. `Resource.make` acquires a value and registers a
release that sees the outcome, `Scope.defer` registers cleanup that
sees it, and `defer { … }` / `errdefer { … }` write scope-owned cleanup
directly as statements. `Resource.acquire_release` and `Scope.add_finalizer` stay as the
low-level way to register a release.

## Owning a value

The tour introduces [scoped cleanup](../../tour/scope/),
[rollback on failure](../../tour/release-on-failure/) and [fiber cleanup](../../tour/fibers/).

```flick
# import Resource exposing (Closing)
#
class Close a =
  close : forall t. Closing t -> a ->{Async} Unit
  close_exit : forall t e b. Closing t -> Exit e b -> a ->{Async} Unit
  let close_exit token _ value = close token value
```

```flick
# import Resource exposing (Close)
#
own : forall s a. Close a => a^{} ->{Async, Scope s} a^{s}
# let own = Resource.own
```

`own value` makes the innermost `scope` (or the one of region `s`) the owner
of `value` and gives it back qualified with the region, so it cannot leave
that scope. When the scope ends, `close_exit` runs with the outcome of the
body: an `Exit` of errors and a value the close cannot read, which tells a
success from a failure (`Exit.is_success`) and an interruption
(`Exit.has_interrupts`). An instance that defines only `close` takes the
default `close_exit`, which ignores the outcome.

```flick
import Resource exposing (Close(..), own)

say : String ->{} Unit
let say line = Host.raw_stdout_line line

type Conn = Conn String

instance Close Conn =
  let close _ (Conn name) = say ("close " ++ name)
  let close_exit _ exit (Conn name) =
    say ("close " ++ name ++ (if Exit.is_success exit then ": succeeded" else ": failed"))

main : Unit ->{Async} Unit
let main () = scope {
  let _a = own (Conn "a")
  let _b = own (Conn "b")
  scope {
    let _c = own (Conn "c")
    say "inner body"
  }
  say "outer body"
}
```

```text
inner body
close c: succeeded
outer body
close b: succeeded
close a: succeeded
```

## Order and outcome

A scope closes in phases ([runtime](../runtime/)): it interrupts its
children and waits for them, then runs its finalizers, last registered
first: the closes `own` registers and the releases of `Resource.make` and
`Scope.defer` are among them. So:

- each owned value is closed exactly once, after everything owned or
  acquired into the scope after it, and after the children of the scope
  ended;
- a close runs masked: an interruption of the scope, by `timeout` or a
  closing parent, does not cut it, and it sees the interruption in the
  outcome;
- a close that fails, as with `Effect.die`, adds its defect to the cause of
  the scope, which keeps the cause of the body; the other closes still run.

The same holds on the Effect adapter, on Flick's own runtime and in Go
(`tests/run/runtime/owned_close.fl`, `tests/run/runtime/defer_make.fl`).

## Acquiring and deferring

```flick
make :
  forall r s e a c.
  (Detached r, Fiber.Outlives c s, Fiber.Provided r s) =>
  (Eff r e a)^c -> (forall e2 b. a -> Exit e2 b ->{r} Unit)^c ->{Scope s | r; e} a^{s}
# let make = Resource.make

defer :
  forall r s c.
  (Detached r, Fiber.Outlives c s, Fiber.Provided r s) =>
  (forall e b. Exit e b ->{r} Unit)^c ->{Scope s | r} Unit
# let defer cleanup = Scope.defer cleanup
```

`Resource.make acquire release` runs `acquire` and makes the innermost
`scope` (or the one of region `s`) the owner of the value: when the scope
ends, `release` runs once with the value and the outcome of the body. The
value holds region `s`, so it cannot leave the scope. An interruption does
not come between the acquisition and that registration, and an acquisition
that fails registers nothing: no release runs for a value that was never
acquired, while the values acquired before it are still released.
`Scope.defer cleanup` registers `cleanup` the same way, without a value.
Both take part in the order above, and their work sees the outcome as
`close_exit` does.

A release may use the regions and services that outlive the scope; a
shorter region, or a service provided inside the scope, is `FLICK-G003`,
and a `control` operation, which no handler would answer by then, is
`FLICK-E017`. The compiler decides these constraints at each call; a
function generic in them states them, as `make` does.

```flick
type error Refused = Refused String

say : String ->{} Unit
let say line = Host.raw_stdout_line line

ended : Exit e b -> String
let ended exit = if Exit.is_success exit then "succeeded" else "failed"

type Conn = Conn String

disconnect : Conn -> Exit e b ->{} Unit
let disconnect (Conn name) exit = say ("disconnect " ++ name ++ ": " ++ ended exit)

connect : String ->{Async, Scope s} Conn^{s}
let connect name =
  Resource.make
    (task {
      say ("connect " ++ name)
      Conn name
    })
    disconnect

refuse : String ->{Async, Scope s; Refused} Conn^{s}
let refuse name = Resource.make (task { Fail.fail (Refused name) }) disconnect

session : Unit ->{Async; Refused} Unit
let session () = scope {
  let _a = connect "a"
  Scope.defer (fun exit -> say ("log flushed: " ++ ended exit))
  let _b = connect "b"
  let _c = refuse "c"
  say "not reached"
}

main : Unit ->{Async} Unit
let main () =
  scope {
    let _a = connect "a"
    Scope.defer (fun exit -> say ("deferred: " ++ ended exit))
    let _b = connect "b"
    say "body"
  }
  try session () with
  | Refused name -> say ("refused " ++ name)
```

```text
connect a
connect b
body
disconnect b: succeeded
deferred: succeeded
disconnect a: succeeded
connect a
connect b
disconnect b: failed
log flushed: failed
disconnect a: failed
refused c
```

A closure or a task may keep an acquired value past its scope as long as
it does not use it there; a call that uses it past the scope, such as an
operation of a service on it, is `FLICK-G002`
([regions](../regions/)). Fixtures: `tests/check/pass/regions/made.fl`,
`tests/check/fail/regions/made.fl`.

### Scope-owned `defer` and `errdefer`

Inside an explicit `scope { … }` or `scope owner { … }`, `defer { cleanup }`
and `errdefer { cleanup }` register cleanup statements on that scope's shared
LIFO finalizer stack without writing a `Scope.defer` callback:

- `defer { cleanup }` runs on every close of the owning `scope` (success,
  escaping typed failure, defect, or cancellation);
- `errdefer { cleanup }` runs only when the owning `scope` body ends in failure
  (escaping typed failure, defect, or cancellation), decided once from the
  immutable body-exit snapshot taken before closing begins.

| Body outcome of owning `scope` | `defer { … }` | `errdefer { … }` |
| - | - | - |
| Succeeds (including returning `Err _` or `None` as data) | runs | skipped |
| Typed failure caught inside the `scope` (`try … with`) | runs | skipped |
| Escaping typed failure, defect, or cancellation | runs | runs |
| Finalizer defect or cancellation arriving during close of a successful body | remaining run | skipped (snapshot stays success) |

`defer` and `errdefer` are contextual statement heads only before `{`, never
first-class value expressions (`let x = defer { … }` is rejected; to call a
function named `defer` with a record, write `(defer) { … }`). Owner selection
binds lexically to the nearest explicit enclosing `scope`, passing through
blocks, `if`, `match`, `loop`/`while`/`for … do`, `try` branches, and `provide`,
and stopping at functions, lambdas, `task`, `lazy`, comprehensions, effect
handler arms, and cleanup bodies themselves. Ambient runtime owners and generic
`Scope s` helpers are not owners for this syntax; use `Scope.defer` there.

The cleanup block must have type `Unit` (returning a cold `Eff` is rejected)
and an empty unhandled error row (`FLICK-E002`; handle typed errors locally
with `try … with`). Like `Scope.defer`, it enforces `Detached` (`FLICK-E017`),
`Fiber.Outlives`, and `Fiber.Provided` (`FLICK-G003`), and does not mask
acquisition: if acquiring a resource and registering its release must be
atomic against cancellation, use `Resource.make`.

```flick
type error Failed = Failed String

say : String ->{} Unit
let say line = Host.raw_stdout_line line

step : Bool ->{; Failed} String
let step should_fail = scope {
  defer { say "close" }
  errdefer { say "rollback" }
  if should_fail then Fail.fail (Failed "boom") else "ok"
}

main : Unit ->{} Unit
let main () =
  say (try step false with | Failed msg -> msg)
  say (try step true with | Failed msg -> msg)
```

```text
close
ok
rollback
close
boom
```

```flick fail G004
say : String ->{} Unit
let say line = Host.raw_stdout_line line

outside : Unit ->{} Unit
let outside () =
  defer { say "no explicit scope" }
```

## Only the owner closes

A close takes a `Closing t`, which only `own` makes, for the close it runs:
a program has no value to call `close` with, so it cannot close an owned
value early (`FLICK-T001`). The token names a region that only that close
keeps live: a close cannot keep it in a cell or a closure and close another
value later (`FLICK-T008`, `FLICK-G002`).

`own` takes a value that holds no region (`a^{}`): a value a scope owns
already, or data that holds one, would be closed twice or by a scope that
ends before the one that uses it.

```flick fail G001
import Resource exposing (Close(..), own)

type Conn = Conn (Unit ->{} Unit)

instance Close Conn =
  let close _ _ = ()

twice : Unit ->{Async} Unit
let twice () = scope {
  let conn = own (Conn (fun () -> ()))
  ignore (own conn)
}
```

`FLICK-G001`: the value holds region `s`, but the expected type holds no
captures.

## Services of an acquisition

The acquisition of `Resource.manual` and `Effect.resource_auto` runs with
the services of the call, in a region `t` of its own. It assumes
`Fiber.Provided r t`: a finalizer it registers there, such as the release
of `Resource.acquire_release`, may use those services, whose providers
outlive `t` as they outlive the resource's region.

## Pools, transactions and servers

These resources of the standard library decide by how the work that held
them ended, as a `close_exit` does:

| Resource | Success | Typed error | Defect | Interruption |
| - | - | - | - | - |
| an item of `Pool.get` or `Pool.use` | back to the pool | back to the pool | released, the pool acquires another | released, the pool acquires another |
| `Database.transaction` | committed | rolled back | rolled back | its connection is closed, and the server rolls back |
| `Http.Server.serve`, `HttpApi.listen` | drained | drained | drained | drained |

A defect or an interruption may leave a pooled item, such as a connection,
broken in the middle of its work, so the pool does not lend it again;
`Pool.invalidate` still drops an item by hand. A transaction whose `COMMIT`
loses its answer fails with the code `"commit_unknown"`
([sql](../sql/)). However the scope of a server ends, it stops accepting
connections and waits for the requests in flight and their finalizers, at
most the graceful shutdown timeout. Run cases:
`tests/run/stdlib/effect/pool_outcomes.fl`,
`tests/run/stdlib/http/drain_outcomes.fl` and, against PostgreSQL,
`tests/postgres/outcomes.fl`.

## Cleanup and control effects

A finalizer of a scope, the release of `Resource.acquire_release` and the
close of an owned value run when the scope closes, after its handlers may
have let the body go, so they perform no `control` or `abort` operation
(`Detached`, `FLICK-E017`). The cleanup of `Effect.ensuring`,
`Effect.on_exit`, `Effect.on_interrupt` and the release of
`Resource.bracket` runs in the fiber of the body, right after it, and may:

- when the body ended with a value, a typed error or a defect, the cleanup
  runs as any code, and its operations are answered (if a defect and an
  `abort` combine, the defect wins cleanly without being swallowed by the
  `abort` handler);
- when the body was interrupted, or an `abort` (or a `control` arm that let
  it go) unwinds through it, a `control` operation of a handler around the
  cleanup is cut off: its arm does not run and the cleanup stops there,
  while the interruption or the unwind goes on as it was. A handler the
  cleanup installs itself still answers.

```flick
effect Emit =
  control emit : String ->{} Unit

effect Stop =
  abort stop : forall b. String ->{} b

say : String ->{} Unit
let say line = Host.raw_stdout_line line

collect : forall r e. (Unit ->{Emit | r; e} Unit) ->{r; e} List String
let collect body =
  try body () with
  | Emit.emit x -> [x] ++ resume ()
  | return () -> []

stopping : forall r e. (Unit ->{Stop | r; e} Unit) ->{r; e} String
let stopping body =
  try
    body ()
    "no stop"
  with
  | Stop.stop why -> "stopped: " ++ why

cleanup : Eff {Emit} none Unit
let cleanup = task {
  say "cleanup begins"
  Emit.emit "cleanup"
  say "cleanup ends"
}

main : Unit ->{} Unit
let main () =
  say (show (collect (fun () -> Effect.ensuring cleanup (task { say "body" }))))
  say (show (collect (fun () -> say (stopping (fun () -> Effect.ensuring cleanup (task { Stop.stop "why" }))))))
```

On Go and Flick's own runtime this prints the following; the Effect adapter
runs no `control` arm (`FLICK-P012`):

```text
body
cleanup begins
cleanup ends
["cleanup"]
cleanup begins
stopped: why
[]
```
