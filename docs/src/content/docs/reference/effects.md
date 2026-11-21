---
title: "Effects and errors"
---

How the checker decides where a runtime call may run, which services and
errors its rows hold, and how handlers remove them (see [Direct effectful calls](../../spec/01-direct-calls/), [Values of computations and their execution](../../spec/02-computations/), [Errors, services, resources and concurrency](../../spec/05-errors-and-concurrency/)).
Services, layers and the host services are in [services](../services/); types and kinds are in [types.md](../types/).

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
`flick fmt` omits empty rows in arrows: `->{none; E}` becomes `->{; E}`,
`->{Log; none}` becomes `->{Log}`, and `->{none; none}` becomes `->{}`.
It also drops a trailing `;` before omitted errors. Generated signatures use
the same short forms. Elsewhere `none` stays, as in `Eff none none a`.

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
  signature. The error points at `h` and offers to annotate it,
  `fun (h : Unit ->{Log} Unit) -> h ()`.
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

A function can take its argument unstarted itself: a parameter marked `~`,
`~(Eff r e a)`, receives a runtime call written there as the computation
`task { … }` would make of it. The standard library's combinators that run
a computation mark it, so `Effect.or_die (Database.query q)`,
`timeout 500 (Http.get url)` and `Effect.retry policy (fetch id)` need no
`task`; an `Eff` value passed there is passed as it is.

```flick
twice : ~(Eff r e Int)^c ->{r; e} Int
let twice job = await job + await job
```

- `twice (count ())` runs `count ()` twice, once per `await`.
- The marker is not part of the type: it belongs to the signature, travels
  in the module's interface, and works where the function is called by its
  name. A pipe `x |> f` evaluates `x` first, as ever.
- `~` goes only on a parameter of type `Eff` (`FLICK-T076`).

## `lazy`

`lazy { body }` builds a `Lazy r e a`: like `task`, it runs nothing yet, but
its first `await` runs the body, in the fiber that awaits, and every later
`await` gets the same outcome without running it again: the value, the typed
failure, or the defect. `lazy e` is `lazy { e }`.

```flick
# type error Missing = Missing
# service Users =
#   find : Int ->{; Missing} String
names : Int ->{Users, Async; Missing} (String, String)
let names id =
  let name = lazy { Users.find id }
  (await name, await name)
```

- Building a lazy allocates its cell, as `Ref.make` does, so it is a runtime
  call (`FLICK-E001` in a pure function).
- `await` of a `Lazy r e a` needs `{Async | r}` and fails with `e`: another
  fiber may be running the body, and then the `await` waits for it. Fibers
  that await one lazy at once share one run.
- An interruption is not kept. When the fiber running the body is
  interrupted, the next `await` runs the body again; the fibers waiting for
  it see no interruption.
- The body runs synchronously in the fiber that forces the lazy, so an `abort`
  operation can unwind to a handler in that fiber (resetting the cell), while a
  `control` operation cannot split it: `lazy` requires `NoControl r`
  (`FLICK-E017`), and every `Detached r` satisfies `NoControl r`.
- A lazy that is never awaited never runs its body.
- A `Lazy` keeps what its body captured, and also the error it failed with,
  so it cannot outlive the `scope` of a resource it holds.

`let lazy x = e` binds a lazy of its function alone, as `let mut` binds a
variable: it is read only as `await x`, which runs `e` the first time and
gives its outcome after that, and needs no `Async`, since no other fiber
can reach it. A closure or a `task` cannot use it (`FLICK-G005`); share one
with `lazy { … }`. A `let lazy mut` can be assigned, and a value assigned
before the first `await` means `e` never runs.

```flick
# service Config =
#   load : Unit ->{} String
banner : Bool ->{Config} String
let banner verbose =
  let lazy config = Config.load ()
  if verbose then await config else "quiet"
```

`docs/formal/lazy.md` gives the formal rules. In Lean, `lazy` and `await` of
a `Lazy` are part of the typed core, so its soundness theorems cover them, and
`formal/Formal/Lazy.lean` proves that an initializer starts at most once
between two resets and that a settled cell keeps its outcome.
`formal/Formal/RT/LazyCell.lean` proves the single flight of the list above
(R9) over every interleaving of a cell's steps. `let lazy` and `~`
parameters are outside those proofs, and none of them proves the compiler or
a runtime.

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

A `match` takes a value of `Errors e` apart by the errors of its row: an arm
`NotFound id` matches the `NotFound` of the row. The arms without a guard
cover every error of a closed row, or the `match` is `FLICK-T009`, whose fix
adds the missing arms; a row with a variable also needs an arm for the rest,
and an arm for an error outside a closed row is `FLICK-T003`. The `match` is
pure and removes no error from a row. When the errors of the value are not
known yet, as for the variable of a `try`, the patterns tell them, at the top
of an arm or inside it: the row holds the errors they name, and stays open
for the others when an arm without a guard matches every error:

```flick
# type alias UserId = String
# type error NotFound = NotFound UserId
# type error DbUnavailable = DbUnavailable
# service Users =
#   get : UserId ->{; NotFound, DbUnavailable} String
#
problem : Errors {NotFound, DbUnavailable} -> String
let problem e =
  match e with
  | NotFound id -> "no user " ++ id
  | DbUnavailable -> "try again later"

name_or_problem : UserId ->{Users} String
let name_or_problem id =
  try Users.get id with
  | e -> problem e
```

A value of one error goes where the errors of a row that holds it are
expected: in an arm of a `match`, as the result of a function or as an
argument. A function can so turn one row of errors into another, as
`Exit.map_error` and `Layer.catch` take. This holds of the value, not of a
type built of it: a function returning `NotFound` is no function returning
`Errors {NotFound, DbUnavailable}`, so write `fun x -> f x` there. Where the
expected row is not known yet, the first error makes it the row of that
error alone; write the row where the value may be one of several:

```flick
# type alias UserId = String
# type error NotFound = NotFound UserId
# type error DbUnavailable = DbUnavailable
# type error Http404 = Http404 UserId
# type error Http503
#
to_http : Errors {NotFound, DbUnavailable} -> Errors {Http404, Http503}
let to_http e =
  match e with
  | NotFound id -> Http404 id
  | DbUnavailable -> Http503

mapped : Exit {NotFound, DbUnavailable} String -> Exit {Http404, Http503} String
let mapped exit = Exit.map_error to_http exit
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

An effect may take type arguments, as `Ask a` below. A handler answers one
instance, the one its arms are typed at: `Ask String` in `with_name`. It
finds an operation by its name, so the body may perform `Ask` only at that
instance, and a row variable of the body that may hold `Ask` at other
arguments is `FLICK-E019`: take it out, `{-Ask | r}`, or name the instance,
`{Ask String | r}`, as `with_name` does. `Ask Int` and `Ask String` stay
apart through substitution, under data and in polymorphic arguments, and
`-Ask Int` is `FLICK-T002`: `-` takes out every instance of a name.

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

A task that `spawn`, `parallel`, the races, `timeout`, `Effect.cached`, the
concurrent collection combinators (`Effect.for_each`, `Effect.all`,
`Effect.partition`, `Effect.validate`, `Effect.filter`,
`Effect.filter_map_effect`), or the runs of `FiberSet`, `FiberMap` and
`FiberHandle` start runs in a fiber of its own. The handler of an effect with a
`control` or `abort` operation is in the fiber that installed it, so such a
task needs `Detached r`: no effect of its services has either. Work a region
keeps needs it too: a finalizer, the release of a resource, a listener. It runs
as its scope closes, when a handler may already have let the body go, and
neither `control` nor `abort` can unwind back to it. A `lazy` initializer runs
synchronously in the forcing fiber, so it needs only the superclass
`NoControl r` (no `control` operation; `abort` is allowed). The compiler
decides `Detached r` and `NoControl r` from the row; a function that starts or
keeps detached work with the services of a row variable names `Detached r` in
its signature, and one that only builds a `lazy` names `NoControl r`. A
disallowed `control` or `abort` effect there, or a row variable without the
constraint, is `FLICK-E017`:

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
coroutine: Go and Flick's own JavaScript runtime (`[target.js] runtime = "rt"`) run it,
while the Effect adapter (`[target.js] runtime = "effect"`) does not and reports
`FLICK-P012`.
