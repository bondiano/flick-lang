---
title: "Effects, errors and services"
---

How the checker decides where a runtime call may run and which services and
errors it needs (see [Direct effectful calls](../../spec/01-direct-calls/), [Values of computations and their execution](../../spec/02-computations/), [Errors, services, resources and concurrency](../../spec/05-errors-and-concurrency/)). Types and kinds are in [types.md](../types/).

## Stages

A runtime call runs when the last argument of a runtime arrow is supplied.
Each arrow of a curried function is a *stage*: `A -> B ->{R; E} C` is a pure
partial application followed by a runtime call, `A ->{R; E} (B -> C)` runs
after the first argument. A function body runs at its last stage, so its
calls count against the last arrow of its signature.

```flick
# service Log =
#   info : String ->{} Unit
#
greet : String -> String ->{Log; none} String
let greet prefix name =
  Log.info name
  "\{prefix}\{name}"

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
bound for them (see [Notation and joining of rows](../../spec/03-types-and-classes/#notation-and-joining-of-rows)): a body may need fewer
services and errors than its signature allows, never more. What exceeds the
bound is `FLICK-E002`, which lists every extra service and error:

```flick
# type UserId = UserId String
# type alias User = { name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# service Log =
#   info : String ->{} Unit
#
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
# service Log =
#   info : String ->{} Unit
# each : (a ->{r; e} Unit) -> List a ->{r; e} Unit
# let each f xs = List.each_effect f xs
#
log_all : List Float ->{Log; none} Unit
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
(see [Unit, Never and empty rows](../../spec/02-computations/#unit-never-and-empty-rows)). It fits every row kind, so the same
`none` is a service row, an error row or a capture row by where it stands.
Every spelling below is the same closed row with no elements and no tail:

| Written | Where |
| - | - |
| `none` | any row position: `->{none; E}`, `Eff none none a`, `Layer none none {Clock}` |
| nothing | one side of an arrow: `->{; E}` has no services, `->{R}` no errors |
| `->{}` | an arrow with both rows empty |
| `{}` | a braced row: `Eff {} {} a` |

In an arrow the side before `;` is always the service row and the side after
it the error row, so `->{; AppError}` and `->{none; AppError}` are one type,
and without `;` the row is always services: `->{Log}` means `->{Log; none}`.
To declare errors, keep `;`, as in `->{; AppError}`.
`flick fmt` drops a trailing `;` before omitted errors and writes an empty service row of an
arrow as nothing, `->{none; E}` becomes `->{; E}`, and an arrow with both rows
empty as `->{}`. Elsewhere `none` stays, so `->{Log; none}` and
`Eff none none a` are left as they are.

```flick
# service Log =
#   info : String ->{} Unit
#
fail : String ->{; AppError} a
# let fail message = App.fail message

log_line : String ->{Log} Unit
# let log_line line = Log.info line
```

An empty row does not make a call pure. `a ->{} b` is still a runtime arrow:
it may do IO, keep state, die with a defect or be interrupted, it only asks
for no services and declares no expected errors. It is not `a -> b`: calling
it where only pure code runs is `FLICK-E001`, passing it where `a -> b` is
expected is `FLICK-T003`. A pure arrow has no rows at all, which is not the
same as two empty ones.

`none` is not a type. It is neither `Unit` (one value), `Never` (no values)
nor the `None` of `Option`: `x : none` and `none -> Float` are `FLICK-T002`,
and so is `Eff Unit Unit a`, because `Unit` is a `Type` and not a row. Inside
a row `none` stands for the whole row, like a row variable: `{Log, none}` is
`FLICK-T002`, write `{Log}`.

The checker relates `none` to the rest as follows:

- `Errors none` is `Never`; when `Errors e` with an unknown `e` meets
  `Never`, `e` becomes `none`.
- A signature is an upper bound, so a body that needs nothing fits any row,
  and a function with `none` fits wherever more services or errors are
  allowed.
- Only `->` and `Eff` relate by rows. Under any other constructor the rows
  must be equal: `List (Eff {Log} none Unit)` is not a
  `List (Eff {Log, Clock} none Unit)` (`FLICK-T003`).
- A parameter of unknown type applied to an argument is taken as a pure
  function, so `(fun h -> h ()) f` with an effectful `f` is `FLICK-T003`.
  An effectful callback gets its type from the expected type or from a
  signature.
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

`Effect.void : Eff none none Unit` is a reusable cold computation that succeeds
with `()`, needs no services and has no expected errors. Use it when an `Eff`
argument should do nothing; `()` alone is a value, not a computation.
`await Effect.void` runs it, and the same value can be awaited again or passed
to consumers such as `Effect.all Effect.Sequential [Effect.void, Effect.void]`.

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
RetryExhausted e Float` keeps the last cause, `type error SendFailed a =
SendFailed a` hands back the value that was not sent. A row tells its errors
apart by name, not by arguments, so it holds at most one `RetryExhausted`:
two of them in one row must have the same arguments (`FLICK-T003`). The
TypeScript projection has the same shape, since the `_tag` of an error does
not depend on its arguments: a union of errors holds each tag once and
`catchTag` narrows it to one type.

`try … with` handles the errors of its body. An error leaves the row only
when an arm without a `when` guard matches every value of it; a guarded arm
or a partial pattern leaves it possible. A variable or `_` matches every
error, as a value of `Errors e`, where `e` holds every error of the body: it
is used as one error only when the body raises only that one. What the
handlers need is added:

```flick
# type UserId = UserId String
# type alias User = { name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# type error DbUnavailable = DbUnavailable
# find_user : UserId ->{Users; NotFound, DbUnavailable} User
# let find_user id = Users.get id
#
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
# type alias User = { name : String }
# type error NotFound = NotFound String
#
checked : Result NotFound User ->{; NotFound} User
let checked result =
  from_result result
```

`Fail.fail` fails with an error directly. The second row of an arrow is the
effect `Fail`: `->{Fail {NotFound}; }` is `->{; NotFound}`, and an arm of a
`try` catches it, written `| NotFound _ ->` or, in full,
`| Fail.fail (NotFound _) ->`:

```flick
# type error NotFound = NotFound String
#
find : String ->{Fail {NotFound};} Int
let find name = Fail.fail (NotFound name)

found : String ->{} Int
let found name =
  try find name with
  | Fail.fail (NotFound _) -> 0
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
# js_now : Unit ->{} Float
# let js_now () = 0.0
#
service Clock =
  now : Unit ->{} Float

clock_impl : Clock.Impl
let clock_impl =
  { now = js_now }

clock_layer : Layer none none {Clock}
let clock_layer =
  Layer.succeed Clock clock_impl
```

A `type alias` of a service or of an error stands for it in a row:
`type alias Time = Clock` makes `->{Time}` the same row as `->{Clock}`.
An alias of a whole row is a [set](#sets-of-services-and-errors). An alias of
anything else in a row is an error (`FLICK-T002`). An alias
names a type, not a namespace: as an alias of a data type does not open its
constructors, an alias of a service does not open its operations or
`S.Impl`. A [re-export](../modules/) does: `Atom.Registry`, which `Atom`
re-exports from `Reactivity.Atom`, is that service, so `Atom.Registry.get`,
`Atom.Registry.Impl` and `Layer.succeed Atom.Registry impl` all work.

## Sets of services and errors

A `type alias` of a row names a set of services and errors. `...` merges a set
into a row, a plain name adds one element, and `-` takes one out:

```flick
# type UserId = UserId String
# type alias User = { name : String }
# type error UserNotFound = UserNotFound
# type error InvalidEmail = InvalidEmail
# type error OrderNotFound = OrderNotFound
# type error Boom = Boom
# service Log =
#   info : String ->{} Unit
# service Users =
#   get : UserId ->{; UserNotFound} User
# service Billing =
#   charge : Float ->{} Unit
#
type alias UserErrors = {UserNotFound, InvalidEmail}

type alias Reads = {Log, Users; ...UserErrors}

type alias Orders = {...Reads, Billing; OrderNotFound}

type alias Quiet = {...Orders, -Log; -InvalidEmail}

load : UserId ->{...Orders} User
let load id = Users.get id

wider : UserId ->{...Orders, Async | r; Boom} User
let wider id = load id
```

A set expands where it is named: `->{...Reads}` is the same row as
`->{Log, Users; UserNotFound, InvalidEmail}`. That gives composition without
any rule of its own:

- Sets may name each other in any order, and an element two sets hold counts
  once. A set cannot hold itself (`FLICK-R007`).
- What needs less fits where more is allowed, as for
  [any row](#rows-a-body-needs): a function of `{...Reads}` passes where one of
  `{...Orders}` is expected. Under another constructor rows stay equal, so
  `List (Eff {...Reads} none Unit)` is not a
  `List (Eff {...Orders} none Unit)` (`FLICK-T003`).
- An arm of a `try` catches one error of a set, and the rest stay in the row.

In braces a set takes `...` and one element does not, both `FLICK-T002` with a
fix. Outside braces an alias is the whole row: `Layer none none Services`,
`Eff Services Errs a`, `Services + r`. A row that merges or takes out a set may
leave out its `;`, since a set can hold errors of its own.

A set of errors alone stands on either side of `;`; a set with services only
before it (`FLICK-T002`). An arrow and `Eff` take the errors of a set into their
row of errors. Any other type with a row, such as `Layer {...Orders} none {Log}`,
has no place for them (`FLICK-T002`): name the services in an alias of their own.

`-` takes from everything the braces merge, wherever it stands, and services and
errors each go on their own side of `;`. `-...Reads` takes a whole set. All of
these are `FLICK-T002`, so a typo or a rename does not pass silently:

- taking out what the row does not hold;
- taking an element with arguments out of a row variable, as in
  `{-Store Int | r}`: the variable may hold `Store` at another type, so write
  `{Store Int | r}` and leave `r`;
- writing an element and taking it out in one row, as in `{Log, ...Reads, -Log}`;
- `...r`, since a row variable goes after `|`; merging an alias with a tail, since
  `| r` belongs where the set is used; `opaque type alias` of a row.

Over a row variable `-` says what a function gives on its own. `{-Log | r}` is
`r` without `Log`, whether or not `r` holds it, so the caller owes everything of
its row but `Log`:

```flick
# service Log =
#   info : String ->{} Unit
# service Clock =
#   now : Unit ->{} Float
#
quiet : Layer none none {Log} -> Eff r none a ->{-Log | r} a
let quiet layer work = provide layer { await work }

timed : Layer none none {Log} -> Eff {Clock, Log} none a ->{Clock} a
let timed layer work = quiet layer work
```

Without `provide`, the body of `quiet` would need `Log` whenever `r` holds it,
which the signature does not allow (`FLICK-E002`). Errors work the same after
`;`, as in `->{; -NotFound | e}` over a `try` that handles `NotFound`.

A set may take parameters, as in `type alias Db s = {Scope s, Log}`.

A set widens a signature: its callers must provide all of it, and tests must too.
Use sets at the boundaries of modules, where many functions need the same things.
Leave a small function its exact row, or let the row be inferred.

## Layers and `provide`

`Layer inputs errors outputs` builds the services `outputs` from `inputs` and
may fail with `errors`. `provide layer { body }` builds the layer before the
body runs: the body may need the services the layer builds, and the block
needs the layer's inputs and errors besides the rest of what the body needs.

```flick
# type UserId = UserId String
# type alias User = { name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# service Log =
#   info : String ->{} Unit
# service Config =
#   url : Unit ->{} String
# type error ConnectError = ConnectError
# users_live : Layer {Config} {ConnectError} {Users}
# let users_live = Layer.effect Users (task { from_result (Err ConnectError) })
# load_name : UserId ->{Users, Log; NotFound} String
# let load_name id = (Users.get id).name
#
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

The services a `provide` builds end with it. Work an owner keeps until its
scope closes (a child of `Fiber.spawn`, `FiberSet.run`, a finalizer of
`Scope.add_finalizer`, a server's handler) runs with the services of the
place that started it, so each of them must come from a provider that
outlives that owner: a `provide` around the owner's `scope`, or the caller
of a function whose body opens that `scope`. Otherwise it is `FLICK-G003`:

```flick fail G003
# service Greeter =
#   greet : String ->{} String
# report : Failure e ->{} Unit
# let report _ = ()
# greeter : Layer none none {Greeter}
# let greeter = Layer.succeed Greeter { greet = fun name -> name }
#
leaked : Unit ->{} Unit
let leaked () = scope outer {
  provide greeter {
    // FLICK-G003: the child of `outer` would use `Greeter` after the
    // `provide` released it
    ignore (Fiber.spawn outer report (task { Greeter.greet "child" }))
  }
}
```

A function whose owner and services both come from its caller says that
the services outlive the owner with `Fiber.Provided`; each call checks it:

```flick
# service Greeter =
#   greet : String ->{} String
# report : Failure e ->{} Unit
# let report _ = ()
#
start : forall s. Fiber.Provided {Greeter} s => Fiber.Owner s ->{Greeter} Unit
let start owner = ignore (Fiber.spawn owner report (task { Greeter.greet "child" }))
```

## What may suspend: `Async`

A function that waits for time, input or output or another fiber says so
with the effect `Async` in its row: `Effect.sleep : Duration ->{Async} Unit`,
`Database.query`, `Http.Client.get`, `Fiber.join`, `Queue.take`. A row
without it performs no asynchronous wait. Ordinary runtime calls and recursive
loops still have interruption points, including `->{}`; an empty row does not
promise termination or atomic execution. Pure calls and intrinsic UI Sync stages
remain synchronous. `Async` has no operations: the scheduler of the
runtime handles it at the root, so `main` may have it, and so may the tasks
of `spawn` and `parallel`; `provide Async` is `FLICK-E016`.

```flick
wait : Float ->{Async} Unit
foreign wait from "./timers.mjs" "wait"

twice : Unit ->{Async} Unit
let twice () =
  wait 1
  wait 2
```

A `foreign` whose row has `Async` is a call whose JS result is a promise;
without it, a call that returns at once. A `foreign effect` is as written:
without `Async` it runs at once, and a suspension is a defect. A call that may suspend in a row without `Async`
is `FLICK-E002`, whose fix adds `Async` where the row is written: the
signature, an `Eff`, or the row given to a type such as `Handlers none`.

## Explicit host dependencies

`Host` provides five ordinary services: `Host.Console` (standard streams),
`Host.Clock` (wall and monotonic time reads), `Host.Random` (a draw in `[0, 1)`),
`Host.Environment` and `Host.Arguments`. None has an implicit implementation.
Provide `Host.console`, `Host.clock`, `Host.random`, `Host.environment` or
`Host.arguments` at the entry boundary; `Host.live` combines all five.

```flick
mode : Unit ->{Host.Environment} Option String
let mode () = Host.Environment.env "MODE"

selected : Unit ->{} Option String
let selected () =
  let environment = Layer.succeed Host.Environment { env = fun _ -> Some "test" }
  provide environment { mode () }
```

Service implementations are records, so tests can capture output, fix time and
draws, or supply arguments without changing process globals. Tasks read their
services where they run; an inner provider restores the outer one when it ends.
The live clock reads the ambient fiber clock; replace scheduling time with
`Test.clock_test`, rather than expecting a custom `Host.Clock` to drive timers.

The existing `Platform`, `Clock`, `Random`, `Console` and `DateTime.now` APIs
remain trusted ambient adapters for compatibility. Their empty rows do not
identify host capabilities. Use `Host` in application contracts that need this
guarantee; this API is not a sandbox against foreign code.

## Effects and handlers

`effect E params =` declares an effect: its operations are called as those
of a service, `E.op x`, and a call needs `E` in the row of its stage. Before
its name an operation may say how a handler's arm for it returns:

- `tail`, the default: the arm is the value of the operation;
- `abort`: the operation never returns, so its result is a variable of its
  own, `forall b. … -> b` (`FLICK-E014`), and the arm is the value of the
  handler;
- `control`: the arm is the value of the handler, and `resume` is the rest
  of the body, which gives back the handler's value.

A service is an effect whose operations are all `tail`: another class in a
`service` is `FLICK-E013`. The words `effect`, `tail`, `abort` and `control`
are keywords only there.

`try … with` handles operations as it handles errors. An arm
`| E.op p… -> h` takes the arguments of a call; `| return p -> h` takes the
value of the body, and the handler's value is then `h`. A handler has one
`return` arm, without a guard, matching every value (`FLICK-E015`); without
it the body's value is the handler's. An effect leaves the row of the body
when arms without a guard take every argument of each of its operations;
other calls of it go on to the handler around. The arms run where the
handler is: what they need is the enclosing stage's, and a failure of a
`tail` arm is the handler's, which no `try` in the body sees.

```flick
effect Ask a =
  ask : Unit ->{} a

effect Emit a =
  control emit : a ->{} Unit

effect Throw =
  abort throw : forall b. String ->{} b

with_name : forall a r e. String -> (Unit ->{Ask String | r; e} a) ->{r; e} a
let with_name name body =
  try body () with
  | Ask.ask () -> name

collect : forall a r e. (Unit ->{Emit a | r; e} Unit) ->{r; e} List a
let collect body =
  try body () with
  | Emit.emit x -> [x] ++ resume ()
  | return () -> []

attempt : forall a r e. (Unit ->{Throw | r; e} a) ->{r; e} Result String a
let attempt body =
  try Ok (body ()) with
  | Throw.throw message -> Err message
```

An `abort` closes the scopes it leaves before its arm runs. A `control`
arm that ends without calling `resume` lets the rest of the body go, which
closes its scopes too; calling `resume` a second time is a defect.
`resume` holds the handler's region, so it stays in the arms: kept in a
cell made outside, it is `FLICK-G002`:

```flick fail G002
# effect Emit a =
#   control emit : a ->{} Unit
# zero : Unit ->{} Int
# let zero () = 0
#
escaped : (Unit ->{Emit Int; none} Int) ->{} Int
let escaped body =
  let cell = Ref.make zero
  try body () with
  | Emit.emit _ ->
    // FLICK-G002: `resume` stays in the arms of its handler
    Ref.set cell resume
    0
```

`provide S impl { body }` handles the service `S` in its body with the
record `impl : S.Impl`, each operation by its field, as `tail` arms do. The
record is named, or written in parentheses: `provide S ({ op = … }) { … }`.
An effect is handled by the arms of a `try` (`FLICK-E016`).

```flick
# service Greeter =
#   greet : String ->{} String
#
greeter : Greeter.Impl
let greeter = { greet = fun name -> "hello, \{name}" }

greeted : Unit ->{} String
let greeted () = provide Greeter greeter { Greeter.greet "ada" }
```

A task that `spawn`, `parallel`, the races, `timeout`, or the runs of
`FiberSet`, `FiberMap` and `FiberHandle` start runs in a fiber of its own.
The handler of an effect with a `control` operation is in the fiber that
installed it, so such a task needs `Detached r`: no effect of its services
has one. Work a region keeps needs it too: a finalizer, the release of a
resource, a listener. It runs as its scope closes, when a handler may already
have let the body go, and no one would answer the operation. The compiler
decides `Detached r` from the row; a function that starts or keeps such work
with the services of a row variable names it in its signature. A `control`
effect there, or a row variable without it, is `FLICK-E017`:

```flick
# effect Emit a =
#   control emit : a ->{} Unit
# collect : forall a r e. (Unit ->{Emit a | r; e} Unit) ->{r; e} List a
# let collect body =
#   try body () with
#   | Emit.emit x -> [x] ++ resume ()
#   | return () -> []
#
both_of : forall r e a. Detached r => Eff r e a -> Eff r e a ->{r; e} (a, a)
let both_of first second = parallel first second

children : Unit ->{} (List Int, List Int)
let children () =
  parallel (task { collect (fun () -> Emit.emit 1) }) (task { collect (fun () -> Emit.emit 2) })
```

A `tail` arm runs on the stack of the caller, which may be Sync, so nothing
it calls may suspend: an effect with a `control` operation, `Async` or a row
variable there is `FLICK-E018`, unless the row of the operation declares it
or a handler inside the arm catches it.

Handlers run on both targets. A `control` arm needs the body to wait as a
coroutine: the Go build runs it, the JavaScript build reports `FLICK-P012`
until its runtime has such a body of its own.

## Not checked yet

| Construct | Today |
| - | - |
| `Layer.merge` used other than applied to both layers | conflicts are not reported |
| a service named as a value with a module qualifier | not accepted; name it unqualified |
