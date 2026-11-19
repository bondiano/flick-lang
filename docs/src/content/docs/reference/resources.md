---
title: "Resources and disposal"
---

A `scope` owns what is acquired into it and releases it when its body ends
([regions](../regions/)). A value of a class `Close` is released through
`Resource.own`: the scope closes it once, with how its body ended, and no
other code can close it. `Resource.acquire_release` and
`Scope.add_finalizer` stay as the low-level way to register a release.

## Owning a value

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
children and waits for them, then runs its finalizers, the closes `own`
registered among them, last registered first. So:

- each owned value is closed exactly once, after everything owned or
  acquired into the scope after it, and after the children of the scope
  ended;
- a close runs masked: an interruption of the scope, by `timeout` or a
  closing parent, does not cut it, and it sees the interruption in the
  outcome;
- a close that fails, as with `Effect.die`, adds its defect to the cause of
  the scope, which keeps the cause of the body; the other closes still run.

The same holds on the Effect adapter, on Flick's own runtime and in Go
(`tests/run/runtime/owned_close.fl`).

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
have let the body go, so they perform no `control` operation
(`Detached`, `FLICK-E017`). The cleanup of `Effect.ensuring`,
`Effect.on_exit`, `Effect.on_interrupt` and the release of
`Resource.bracket` runs in the fiber of the body, right after it, and may:

- when the body ended with a value, a typed error or a defect, the cleanup
  runs as any code, and its operations are answered;
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
