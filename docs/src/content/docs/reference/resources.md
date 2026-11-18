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
  close : Closing t -> a ->{Async} Unit
  close_exit : Closing t -> Exit e b -> a ->{Async} Unit
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
