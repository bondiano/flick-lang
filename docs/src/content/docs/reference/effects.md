---
title: "Effects, errors and services"
---

How the checker decides where a runtime call may run and which services and
errors it needs (see [Direct effectful calls](../../spec/03-direct-calls/), [Values of computations and their execution](../../spec/04-computations/), [Errors, services, resources and concurrency](../../spec/07-errors-and-concurrency/)). Types and kinds are in [types.md](../types/).

## Stages

A runtime call runs when the last argument of a runtime arrow is supplied.
Each arrow of a curried function is a *stage*: `A -> B ->{R; E} C` is a pure
partial application followed by a runtime call, `A ->{R; E} (B -> C)` runs
after the first argument. A function body runs at its last stage, so its
calls count against the last arrow of its signature.

```flick
greet : String -> String ->{Log; none} String
let greet prefix name =
  Log.info name
  prefix ++ name

hello : String ->{Log; none} String
let hello =
  greet "Hello"
```

A runtime call where only pure code runs is `FLICK-E001`: in a function whose
last arrow is `->`, in a callback expected to be pure, and while a module is
initialized. An empty row does not make a call pure: calling an `a ->{} b`
function in a pure function is still `FLICK-E001`, and passing it where an
`a -> b` is expected is `FLICK-T003`. Top-level values are evaluated when the
module loads, so they are built purely; a top-level `task` is fine, its body
is deferred.

`let user = Users.get id` binds the result of the call, a `User`. Arguments
and stages run strictly left to right.

## Rows a body needs

The rows of the calls at a stage are joined, and the signature is an upper
bound for them (see [Notation and joining of rows](../../spec/05-types-and-classes/#notation-and-joining-of-rows)): a body may need fewer
services and errors than its signature allows, never more. What exceeds the
bound is `FLICK-E002`, which lists every extra service and error:

```flick
load_name : UserId ->{Users, Log; NotFound} String
let load_name id =
  let user = Users.get id
  Log.info user.name
  user.name
```

The order of a row does not matter and a repeated element counts once.
`r1 + r2` joins two rows of one kind:

```flick
both : Eff r1 e1 a -> Eff r2 e2 b ->{r1 + r2; e1 + e2} (a, b)
let both left right =
  (await left, await right)
```

A callback takes the rows of the arrow it is checked against; when those are
row variables of the called function, its calls decide them:

```flick
log_all : List Number ->{Log; none} Unit
let log_all numbers =
  each (fun n -> Log.info "number") numbers
```

Where no type is expected, a lambda's last arrow is runtime exactly when its
body makes a runtime call: `fun x -> normalize_name x` is `String -> String`,
`fun x -> task { Users.get x }` is a pure builder of an `Eff`, and
`fun x -> Users.get x` is `UserId ->{Users; NotFound} User`.

A runtime arrow and an `Eff` accept a value that needs fewer services and
errors, and a pure function is accepted where a runtime one is expected (the
compiler lifts it). A runtime function never passes for a pure one.

## Empty rows: `none`

`none` is the literal of the empty row: no services, or no expected errors
(see [Unit, Never and empty rows](../../spec/04-computations/#unit-never-and-empty-rows)). It fits every row kind, so the same
`none` is a service row, an error row or a capture row by where it stands.
Every spelling below is the same closed row with no elements and no tail:

| Written | Where |
| - | - |
| `none` | any row position: `->{none; E}`, `Eff none none a`, `Layer none none {Clock}` |
| nothing | one side of an arrow: `->{; E}` has no services, `->{R;}` no errors |
| `->{}`, `->{;}` | an arrow with both rows empty |
| `{}` | a braced row: `Eff {} {} a` |

In an arrow the side before `;` is always the service row and the side after
it the error row, so `->{; AppError}` and `->{none; AppError}` are one type,
and `->{AppError}` without `;` is a syntax error, never a guess about which
row `AppError` belongs to. `flick fmt` writes an empty service row of an
arrow as nothing, `->{none; E}` becomes `->{; E}`, and an arrow with both rows
empty as `->{}`. Elsewhere `none` stays, so `->{Log; none}` and
`Eff none none a` are left as they are.

```flick
fail : String ->{; AppError} a

log_line : String ->{Log;} Unit
```

An empty row does not make a call pure. `a ->{} b` is still a runtime arrow:
it may do IO, keep state, die with a defect or be interrupted, it only asks
for no services and declares no expected errors. It is not `a -> b`: calling
it where only pure code runs is `FLICK-E001`, passing it where `a -> b` is
expected is `FLICK-T003`. A pure arrow has no rows at all, which is not the
same as two empty ones.

`none` is not a type. It is neither `Unit` (one value), `Never` (no values)
nor the `None` of `Option`: `x : none` and `none -> Number` are `FLICK-T002`,
and so is `Eff Unit Unit a`, because `Unit` is a `Type` and not a row. Inside
a row `none` stands for the whole row, like a row variable: `{Log, none}` is
`FLICK-T002`, write `{Log}`.

The checker relates `none` to the rest as follows:

- `Errors none` is `Never`; when `Errors e` with an unknown `e` meets
  `Never`, `e` becomes `none`.
- A signature is an upper bound, so a body that needs nothing fits any row,
  and a function with `none` fits wherever more services or errors are
  allowed.
- A row the constraints leave unknown at the end of a declaration becomes
  `none`: an inferred function that calls no service gets an empty service
  row, not a free row variable.
- In generated code an empty row is `never` in `Effect<A, E, R>`, as
  [Effect](https://effect.website/docs/v4/getting-started/the-effect-type)
  writes a missing requirement or error.

## `task` and `await`

`task { body }` builds an `Eff r e a` without running it; the calls of the body
count against `r` and `e`. `await t` runs `t` once in the current fiber and
removes exactly one layer: awaiting `Eff r e (Eff r2 e2 a)` gives an
`Eff r2 e2 a`. Awaiting something that is not an `Eff` is `FLICK-T003`.

A function that takes an `Eff` decides when it runs. A runtime call written
as its argument has already run when the function starts: the warning
`FLICK-E003` suggests passing `task { … }` instead.

## Expected errors

`type error E = E T …` declares an error with a nominal identity, its module
and name, and the fields of its one constructor `E`; `type error E` has no
fields. The fields are data that outlive the code that raised the error: a
cell (`Ref`), a computation (`Eff`), a layer, a runtime function or an
`Unknown` in them is `FLICK-E005`, also through the types they use.

An error may take type parameters: `type error RetryExhausted e =
RetryExhausted e Number` keeps the last cause, `type error SendFailed a =
SendFailed a` hands back the value that was not sent. A row tells its errors
apart by name, not by arguments, so it holds at most one `RetryExhausted`:
two of them in one row must have the same arguments (`FLICK-T003`). The
TypeScript projection has the same shape, since the `_tag` of an error does
not depend on its arguments: a union of errors holds each tag once and
`catchTag` narrows it to one type.

`try … with` handles the errors of its body. An error leaves the row only
when an arm without a `when` guard matches every value of it; a guarded arm
or a partial pattern leaves it possible. A variable or `_` matches every
error, as a value of `Errors e`. What the handlers need is added:

```flick
find_optional : UserId ->{Users; DbUnavailable} Option User
let find_optional id =
  try
    Some (find_user id)
  with
  | NotFound _ -> None
```

An arm that matches something other than an error is `FLICK-E007`.

`Errors e` turns an error row into a type: `Errors none` is `Never`,
`Errors {NotFound}` is `NotFound`. `from_result` fails with the error of an
`Err`; the error row of a `Result NotFound a` is `{NotFound}`, and a
`Result String a` is rejected until the string becomes a declared error:

```flick
checked : Result NotFound User ->{; NotFound} User
let checked result =
  from_result result
```

## Services

A `service` lists operations with full signatures. Running an operation looks
the service up, so every operation has a runtime arrow (`FLICK-E006`), and
`S.op` needs `S` at each of them: with `get : UserId ->{; NotFound} User`
declared in `Users`, `Users.get` is `UserId ->{Users; NotFound} User`.
Operations are not class methods: a class method such as `show` needs no
service.

`S.Impl` is the record of the operations as declared. `Layer.succeed S impl`
makes a layer from it; the service is named as a value:

```flick
service Clock =
  now : Unit ->{} Number

clock_impl : Clock.Impl
let clock_impl =
  { now = js_now }

clock_layer : Layer none none {Clock}
let clock_layer =
  Layer.succeed Clock clock_impl
```

## Layers and `provide`

`Layer inputs errors outputs` builds the services `outputs` from `inputs` and
may fail with `errors`. `provide layer { body }` builds the layer before the
body runs: the body may need the services the layer builds, and the block
needs the layer's inputs and errors besides the rest of what the body needs.

```flick
application : UserId ->{Config, Log; NotFound, ConnectError} String
let application id =
  provide users_live {
    load_name id
  }
```

With `users_live : Layer {Config} {ConnectError} {Users}` and `load_name`
needing `{Users, Log}` and `{NotFound}`, the block needs `{Config, Log}` and
`{NotFound, ConnectError}`.

`Layer.merge a b` builds the services of both layers; two layers that build
the same service are `FLICK-E004`. `Layer.override a b` merges them too and
lets `b` replace what `a` builds.

## Not checked yet

| Construct | Today |
| - | - |
| `Layer.merge` used other than applied to both layers | conflicts are not reported |
| a service named as a value with a module qualifier | not accepted; name it unqualified |
