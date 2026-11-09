---
title: "Errors, services, resources and concurrency"
source_hash: 42fc822e9d961d9b
---

## Expected errors

Errors have nominal identity and a payload. `try ... with` handles the selected expected error with native Effect semantics. The payload of every error constructor is region-free: it cannot hold a value of a region (a scoped handle such as `File s`), a cell (`Ref`), a computation (`Eff`), a layer, a runtime function or an `Unknown`, also through the types it uses (`FLICK-E005`, see [Regions and captures](../../reference/regions/#what-cannot-leave-a-scope)). The check applies even to an error that is caught locally. So an error carries a path, an id or a copy of the data, not a live resource.

An error is declared with `type error`. Its single constructor has the same name as the error itself and lists the fields like an ADT variant: `type error NotFound = NotFound UserId`. An error without fields is written `type error Timeout`.

```flick
# type UserId = UserId String
# type alias User = { id : UserId; name : String }
# service Users =
#   get : UserId ->{; NotFound} User
#
type error NotFound = NotFound UserId

load_optional : UserId ->{Users; none} Option User
let load_optional id =
  try
    Some (Users.get id)
  with
  | NotFound _ -> None
```

Given `Users.get : UserId ->{Users; NotFound} User`, the result of `load_optional` is `Option User`, the requirements remain `{Users}`, and the row of selected recoverable errors becomes empty. The full native Cause may hold additional diagnostic failures; its payloads are not passed off as the residual `e`.

An error can have parameters. `type error RetryExhausted e = RetryExhausted e Float` stores the last cause after the attempts are exhausted, and `type error SendFailed a = SendFailed a` returns the value that could not be sent. The error row distinguishes elements by nominal name `Module.Name`, not by arguments, so a row holds at most one element with a given name: the arguments of two `RetryExhausted` in one row are unified, and `RetryExhausted DbError` together with `RetryExhausted String` is a type error (`FLICK-T003`). The TS projection is built the same way: `_tag` does not depend on the arguments, `RetryExhausted<E>` appears in the error union of an `Effect` at most once, and `catchTag` narrows the type unambiguously. A user-defined `retry` could then have this type:

```flick
type error RetryExhausted e = RetryExhausted e Float

retry : Float -> Eff r e a ->{r; RetryExhausted (Errors e)} a
# let retry times body =
#   try await body with
#   | cause -> from_result (Err (RetryExhausted cause times))
```

Removing an error from a row is allowed only when the selected typed recovery fully covers its nominal variant. A guard or a partial pattern on the payload leaves the error possible. The requirements/errors of the handler are added to the result. Row subtraction applies to the selected recoverable error; it does not prove the absence of such a tag among secondary raw failures.

Expected failure, defect and interruption remain distinct outcomes. An empty error row does not guarantee termination, the absence of a defect, or that cancellation is impossible. In Effect, defects and interruption are kept in the runtime termination cause, not in the ordinary parameter `E`. [Kinds of Effect errors](https://effect.website/docs/v4/error-management/two-error-types).

`try ... with` compiles to handling an Effect typed failure; on the Go target, to recovering the cause a failing computation panics with. A JavaScript `try/catch` that surrounds only the construction of an Effect value does not implement this contract.

## Services and type classes

| Mechanism | How the implementation is chosen | What it is for |
| - | - | - |
| Type class | The checker resolves the instance and passes a dictionary | `Eq`, `Show`, `Functor` and other operations over types |
| Service | The implementation is provided for a particular computation | Database, HTTP, logging, a test environment |
| Layer | The runtime builds services from dependencies | Initialization, resource acquisition and composing the environment |

A pure operation `show user` does not require an Effect Context. Replacing a production service with a test one does not change the global choice of a typeclass instance.

In the direct API, `Users.get` is a generated effectful operation: running it looks the service up, so it has a runtime arrow and requires `Users`. On the implementation side, a service may store a method that returns an `Effect`.

Services and errors get the qualified identity of their package and module; a service is the runtime key `M.S`. A service declaration lists the full signatures of the methods; the compiler generates the operations and the type of the implementation record:

```flick
service Time =
  now : Unit ->{} Float

time_impl : Time.Impl
let time_impl =
  { now = Clock.current_time_millis }

time_layer : Layer none none {Time}
let time_layer =
  Layer.succeed Time time_impl
```

`Clock.current_time_millis` is the standard library's reading of the fiber's clock. `Time.now` requires the service `Time`. `Time.Impl` is the record of the operations with their types as declared; a layer that needs a resource acquires it while it is built (`Layer.effect`). Service methods do not become class instances. The details are in [Effects, errors and services](../../reference/effects/#services). [Effect services](https://effect.website/docs/v4/requirements-management/services).

## Providing the environment

The order of a layer's parameters is `Layer inputs errors outputs`. For example:

```text
users_live :
  Layer {Config} {ConnectError} {Users}
```

The block `provide layer { ... }` establishes the environment before the body runs:

```flick
# type UserId = UserId String
# type alias User = { id : UserId; name : String }
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

If `load_name` requires `{Users, Log}` and allows `{NotFound}`, then after providing `users_live` the requirements `{Config, Log}`, the errors `{NotFound, ConnectError}` and the same result `String` remain.

Providing a layer removes the services it supplies, but adds the dependencies and errors of its construction. For an ordinary function, the argument must be `task { ... }` instead of a special block, so that strict evaluation order does not start the body before the environment is established. [Effect Layers](https://effect.website/docs/v4/requirements-management/layers).

## Effects of the program and handlers

`effect E params =` declares an effect: its operations are called as those of a service, `E.op x`, and a call needs `E` in the row of its stage. The declaration gives each operation its class: `tail` (the default), whose handler's arm gives the operation its value; `abort`, which never returns, its arm giving the handler its value; and `control`, whose arm gives the handler its value and gets `resume`, the rest of the body. A service is an effect whose operations are all `tail`.

`try … with` handles operations as it handles errors: an arm `| E.op p… -> …` takes the arguments of a call, `| return p -> …` the value of the body. The arms run where the handler is, and an effect leaves the row of the body when arms without a guard cover all its operations:

```flick
effect Emit a =
  control emit : a ->{} Unit

collect : forall a r e. (Unit ->{Emit a | r; e} Unit) ->{r; e} List a
let collect body =
  try body () with
  | Emit.emit x -> [x] ++ resume ()
  | return () -> []
```

A handler is deep: `resume` goes on with the body under the same handler and gives back its value. `resume` runs at most once, a second call is a defect; an arm that does not call it lets the rest of the body go, and its scopes close. `resume` holds the handler's region, so it does not leave the arms. A task in a fiber of its own (`spawn`, `parallel`, the races) cannot carry an effect with a `control` operation: its handler stayed in the parent's fiber. `provide S impl { … }` handles a service with a record `S.Impl`. The details are in [Effects, errors and services](../../reference/effects/#effects-and-handlers).

## Resources

`scope` combines runtime finalization with a static ban on unsafe escape. Acquisition registers a finalizer with the lexical owner. The safe scoped API has **no early close**; separate manual-handle bindings do not get this guarantee. A library owner that replaces or evicts its resources (`ScopedRef`, `Resource`, `ScopedCache`) hands its value out only under a lease, and releases a value once no lease holds it. With this restriction resources need neither linearity nor typestate for an open/closed handle.

Fresh regions and transitive tracking of captures make it impossible to obtain, through the safe API, a usable capability of an already finished region. A native Cause may physically retain a closed foreign object; the safe wrapper does not expose it. There is no affine/linear type system. The native [Effect Scope](https://effect.website/docs/v4/resource-management/scope) performs finalization on the JS target; the static part belongs to the checker, and the two-phase owner to the runtime adapter ([Runtime adapter](../../reference/runtime/#owners-and-scopes)).

A scoped finalizer has an empty expected-error channel; its failure remains a defect in the full outcome. `Resource.bracket` allows a separate typed release error and adds it to the error row. The two operations are explicitly distinct. The primary error and cleanup are preserved under the native Cause contract; release is not replaced by an ordinary JS `finally` that loses the original failure.

## Concurrency

The main API is the structured `parallel` and `parallel_all` over cold `Eff`s. They cancel the active branches on failure and wait for them to finish. `parallel_all` requires a finite positive integer concurrency limit; success results follow the input order.

Several observed failures are kept in the native Cause; the API does not promise to collect all future errors. `parallel_results` separately accumulates the selected typed errors as `Result`, and `parallel_exits` accumulates opaque full outcomes. Defects and external interruption are not masked by an ordinary successful list.

`Fiber.spawn` requires an explicit lexical owner and supervisor; `Fiber.join` waits for a specific fiber and restores its failure. Child errors are not added to the parent's error row automatically, so a supervisor is mandatory even if the handle may be ignored. The safe API has no unbounded daemon fork with local captures. [Effect Fibers](https://effect.website/docs/v4/concurrency/fibers).

## The boundary of the language core

Retry, schedules, streams, queues, cache, tracing and HTTP are a typed standard library ([Effect surface](../../reference/effect-surface/)). For re-running and concurrency it accepts descriptions of computations. They have no special syntax.

`deriving Schema` and codecs for external formats are part of the language. Generics, recursion, refinements, field policies and effectful transformations are covered in [Schema and standard data types](../07-schema/).

The handlers of a program's effects are one-shot: a continuation resumes at most once, never several times. Services and `provide layer` still rely on Effect's mechanisms, and on the Go target on the same mechanisms of `runtime-go/flickrt`; `control` arms compile for Go, and for JavaScript once it has a runtime of its own.

## Execution and static lifetime contracts

The rules below describe program behavior that stays the same whatever the lowering and the target. More complex modes are separate library operations.

| Area | Contract |
| - | - |
| Typed errors | Nominal, region-free payloads; `e` is the selected recoverable error, so not every raw Fail is a value of `Errors e` |
| Defects and interruption | Kept outside `e`; available through opaque Failure/Exit; an ordinary handler is not a universal catch |
| Provide | Layer construction first, then the body; conflicts are an error unless overridden explicitly |
| Sharing | One Layer identity is shared within one build; sharing between independent provides needs explicit memoization with an owner |
| Scope | Fresh regions/captures; children finish first, then LIFO resource cleanup; closing may wait for uninterruptible or foreign work |
| Cleanup errors | Scoped finalizer without an expected error; typed release via `Resource.bracket`; errors of dynamic finalizers are not merged into `e` |
| Parallel | Fail-fast, cancellation of active branches, waiting for cleanup, input order of results; not every potential failure is collected |
| Spawn | Explicit owner and mandatory supervisor; join restores the failure; the parent's success does not imply the success of background tasks |
| Race/timeout | Native choice of the winner and waiting for losers; no hard deadline and no automatic addition of loser failures |

The Go runtime (`runtime-go/flickrt`) implements the same Cause and Layer semantics: a failing computation carries a cause of typed failures, defects and interruptions that `try`, `Failure` and the entry runner treat by the rules below, and layers are built and shared through a memo map as on the JS target ([codegen, Go target](../../reference/codegen/#go-target)).

## Error as data and error as an execution failure

A pure function can return a `Result` without running an Effect. The source order `Result error value` is convenient for HKT; the runtime representation is the native `Result.Result<A, E>` from [Effect v4](https://effect.website/docs/v4/data-types/result), with the parameters swapped in the TS projection. The parameter `error` here has kind `Type`; the row of runtime errors has kind `ErrorRow`. These are different positions.

```text
parse_config :
  String -> Result ParseError Config

fetch_config :
  Url ->{Http; NetworkError} Config
```

Converting a `Result` to a typed failure is the Prelude function `from_result : Result (Errors e) a ->{; e} a`. For `Result NotFound User` the singleton row `{NotFound}` is used. `Result String a` requires first converting the string to a declared nominal error. The operation does not catch defects or interruption. An ordinary `try ... with` handles expected failures; the separate `Exit`/`Failure` API handles the full outcome. [Separation of Effect errors](https://effect.website/docs/v4/error-management/two-error-types).

```flick
# type UserId = UserId String
# type alias User = { id : UserId; name : String }
# service Users =
#   get : UserId ->{; NotFound} User
#
type error NotFound = NotFound UserId
type error DbUnavailable = DbUnavailable String

find_user :
  UserId ->{Users; NotFound, DbUnavailable} User
# let find_user id = Users.get id

find_optional :
  UserId ->{Users; DbUnavailable} Option User
let find_optional id =
  try
    Some (find_user id)
  with
  | NotFound _ -> None
```

### A complex Cause: what the error row promises

`e` describes **the error available to ordinary typed recovery**. The full native Cause may contain secondary payloads outside the residual `e`; the safe API does not provide `Failure e -> List (Errors e)` for all of its Fails.

Recovery follows Effect 4.1.2, whose Cause is a flat list of `Fail`, `Die` and `Interrupt` reasons ([Cause](https://effect.website/docs/v4/data-types/cause)): `try` handles the first `Fail`; a cause that also holds a `Die` is not recovered and its failures become defects; a cause that is only interruption has no error; without a matching arm the original cause propagates unchanged. The table of cases is in [Runtime adapter, Outcomes](../../reference/runtime/#outcomes). In particular, for `Fail(B), Fail(A)` with an arm for `A` only, the handler is not called: the native `catchTag(A)` narrows E in the TS signature, although a secondary A may remain in the full Cause. So not every raw `Fail` can be passed off as `Errors e`.

Flick keeps the native recovery and an opaque `RawCause`: a handler corresponds to the selected recoverable error, and rethrow preserves the native Cause. A fully typed Cause would stop partial handlers from narrowing `e`.

`Exit e a`, `Failure e` and `RawCause` are opaque types. Inside, the native representation is kept; the typed and diagnostic interfaces are separate.

```text
Failure.primary :
  Failure e -> Option (Errors e)

Failure.raw_cause :
  Failure e -> RawCause

Failure.inspect :
  Failure e ->{} CauseReport

RawCause.inspect :
  RawCause ->{} CauseReport

Failure.rethrow :
  Failure e ->{; e} a
```

A successful `Exit` holds `a` and preserves the captures of that `a`. `Failure.primary` yields only the selected typed error; `rethrow` restores the original Cause with the same `e` or with its permitted extension. There is no safe `RawCause -> Exit e a` for an arbitrary `e`. The operations cannot erase known captures. Typed error payloads are region-free; arbitrary foreign defects remain sealed.

A safe `RawCause` **does not expose the original `JsValue`**. `CauseReport` holds inert kinds of causes, safely obtained tags/text and interruption metadata. Inspection does not call user getters, `toString` or `toJSON`; an unsupported payload is shown as a placeholder. Access to the original JS graph is possible only through an explicit unsafe FFI.

An aggregate error for `parallel` (a `CompositeError`) would change the application API: handling `NotFound` is not the same as handling an aggregate with a NotFound inside. `parallel_results` and `parallel_exits` give the outcomes of all branches instead.

A successful recovery may absorb the cause and its secondary diagnostics. If they are needed in logs, observation is done before the recovery. The rule for selecting an error from the list does not mean that a handler may arbitrarily disable an already requested fiber cancellation: the interruption state belongs to the runtime.

## Lifetime and sharing under provide

`provide` first builds the Layer completely and only then runs the body. If construction fails, the body does not start. Scoped implementations belong to the lexical owner of the provide; on exit the outer environment is restored and a two-phase close is performed. The captures of a service, implementation record, closure, `Eff`, UI or codec are preserved in interfaces and cannot outlive the owner. Work an owner keeps until its scope closes runs with the services of the place that started it, so they must outlive that owner (`Fiber.Provided`, `FLICK-G003`).

`task { service_call () }` looks up the required service at run time in the current environment. If the task has already captured a specific instance of the service, it keeps that instance's lifetime. A finalizer receives the context saved at registration, not an arbitrary service from the environment of a later close.

One Layer identity is reused within one graph build, and a `provide` nested in the body of another reuses the outer build of the same identity. Independent, side-by-side `provide`s build independent instances; there is no global singleton. Long-lived shared construction requires explicit memoization with an owner. Two implementations of one service key in a graph are an error (`FLICK-E004`), except through `Layer.override`. This rule relies on [Effect layer memoization](https://effect.website/docs/v4/requirements-management/layer-memoization), and the checker additionally verifies lifetime.

## Finalization and resource boundaries

```flick
# import File exposing (File, Path, FileSystem, OpenError, ReadError)
#
read_config :
  Path ->{Async, FileSystem; OpenError, ReadError} String
let read_config path =
  scope {
    let file = File.open_scoped path
    File.read_all file
  }
```

`File.open_scoped` requires a lexical `Scope s`, which `scope` provides. After a successful acquisition a finalizer is registered. The result is returned after the children owned by the region finish and the resource cleanup that follows. Resource finalizers run LIFO. Children and resources are separate groups: a resource acquired after a child started is still released after that child ended. [Scope API](https://effect.website/docs/v4/api/effect/Scope).

`Resource.acquire_release` requires a release without expected errors; `Resource.bracket` allows a typed release error. Cleanup defects are preserved in the full outcome; the successful closing of every external resource is not a theorem of the checker.

For safe scoped APIs the checker verifies direct and indirect escape: the result, closures, task, containers, existential wrappers, writes to external state and handing over to a retaining FFI. A resource can be used many times inside its region if its API allows it. Static lifetime by itself does not prove the absence of races or the safety of concurrent use of a handle.

### Static regions and captures

Every `scope s { ... }` creates a fresh region `s` and a runtime owner. In `scope { ... }` the name is inferred and stays hidden. A scoped handle has a type such as `File s`. The owner is passed by the compiler lexically; it is not implemented by a single ordinary Context lookup, which in a nested region could pick a different scope.

```flick fail E002 G001
# import File exposing (File, Path, FileSystem, OpenError, ReadError)
#
bad_deferred :
  Path ->{FileSystem; OpenError} Eff {FileSystem} {ReadError} String
let bad_deferred path =
  scope {
    let file = File.open_scoped path
    task { File.read_all file }
  }
```

This is an error (`FLICK-G001`): the task captures `file`, yet it is being returned after the region has closed. The ordinary type `Eff {FileSystem} {ReadError} String` does not show this dependency, so a check of `s` in the result type alone is not enough.

The correct fix is to move the acquisition inside the deferred execution:

```flick
# import File exposing (File, Path, FileSystem, OpenError, ReadError)
#
deferred_read :
  Path -> Eff {Async, FileSystem} {OpenError, ReadError} String
let deferred_read path =
  task {
    scope {
      let file = File.open_scoped path
      File.read_all file
    }
  }
```

Now every `await` gets its own region and resource. The returned string may outlive the file.

The same check applies to an ordinary callback:

```flick fail E002 G001
# import File exposing (File, Path, FileSystem, OpenError, ReadError)
#
bad_callback :
  Path ->{FileSystem; OpenError} (Unit ->{FileSystem; ReadError} String)
let bad_callback path =
  scope {
    let file = File.open_scoped path
    fun () -> File.read_all file
  }
```

Such a callback also cannot be returned. A higher-rank `forall s` prevents the direct escape of `File s`, but does not prove the absence of capture in a callback, task or generic container without an additional capture system.

| Leak path | Checker requirement |
| - | - |
| A direct result or a nested record/ADT | The region does not outlive the owner; the dependency propagates transitively |
| Closure or Eff | Captures are preserved on passing, generalization and across module interfaces |
| Existential/opaque packaging | Nominality and type hiding do not erase the lifetime |
| Writing to an external Ref or cache | The lifetime of the storage does not exceed the permitted lifetime of the value written |
| Error, Exit or diagnostic value | All exits are checked, including failure payloads; the error path does not bypass lifetime |
| Foreign call | A region-bound argument requires a verified retention contract |

Captures are a separate static property, not a third effect row. The notation `T^{s, t}` specifies concrete regions, `T^c` a row of kind CaptureRow, and `T^(c + d)` their union. An explicit qualifier is an upper bound on the full set of reachable usable captures of the value: the dependencies known from T must be included in it. Widening this bound is allowed, but unsafe narrowing is not. A bare T adds no hidden captures and does not erase intrinsic dependencies: File s and List (File s) depend on s. Merely mentioning s in a function's input type does not mean that its closure holds the resource s; the checker tracks the reachability of a capability, not all the names in a signature.

```text
read_later :
  forall s.
  File s -> (Eff {FileSystem} {ReadError} String)^{s}

scoped_callback :
  forall s.
  File s -> (Unit ->{FileSystem; ReadError} String)^{s}
```

These are interface signatures. Local captures are inferred; public signatures and `.iface` preserve them. Captures pass through partial application, records, opaque/existential types, dictionaries and `Unknown^{s}`. Passing a callback through a generic higher-order API requires a capture-polymorphic contract, like `List.map_effect`. In short scoped examples the region name is omitted in the syntax, but the checker keeps it. Mutable storage is invariant; a longer-lived Ref/cache cannot receive a shorter capability. The full rules are in [Regions and captures](../../reference/regions/).

### Scope of the strong contract

| Property | Contract |
| - | - |
| Using a handle many times in a live region | Allowed if the resource's API permits it |
| Early close | Absent from the safe scoped API |
| Full linearity | Not introduced; lifetime is checked through regions/captures |
| Concurrent access | The binding declares share-safe operations or serializes access; lifetime does not prove the absence of races |
| Cleanup | The runtime runs the finalizers; the compiler does not prove the physical success of an external close |
| Typed errors | The payload is region-free even under local recovery |
| Native Cause | Live foreign payloads are sealed; only an inert report is safely accessible |
| Unchecked JS | Requires an explicit trusted/unsafe contract |

### Fiber owner and release order

A task can capture resources of its own region and of longer-lived regions. Moving to a longer-lived owner is allowed only if there are no short captures (`Fiber.Outlives`, `FLICK-G003`). A subscription or callback retained by an external library also has a verifiable owner.

Closing a scope has a mandatory order: **forbid new work → request cancellation of the children → wait for the terminal state of each → wait for pending foreign work → run resource finalizers LIFO → return the final Exit**. If a child does not finish an uninterruptible section, the close waits. Releasing a resource that is in use under a child that is still running is forbidden; a timeout does not override this rule.

A terminal state of a fiber by itself does not prove that an external Promise operation has stopped. An async FFI with scoped captures must, before completing cancellation, wait for confirmation of the stop or for the actual end of the external work. A lone `AbortSignal` is not enough. If an operation cannot be cancelled, the adapter waits for it to end; detach is allowed only without short captures or with an explicit safe transfer of ownership.

There is no nursery that merges child errors into the scope body's error row. Structured combinators own their branches; a scoped background fiber is cancelled when the owner closes. Errors of background work are always observed by the explicitly passed supervisor, and `join` additionally restores the failure for the waiter.

## Concurrent operations with different contracts

```flick
# type UserId = UserId String
# type alias User = { id : UserId; name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
#
load_pair : UserId -> UserId ->{Users; NotFound} (User, User)
let load_pair first_id second_id =
  parallel
    (task { Users.get first_id })
    (task { Users.get second_id })
```

For `parallel` the order of results matches the order of arguments. A failure stops the launch of further work, requests cancellation of the other active branches and waits for them to finish. The observed non-Interrupt causes are collected, including some failures of the branches being stopped; receiving all possible future errors is not promised.

The operation does not close an external `Scope`: resources registered in it live until it is closed. If a resource must close together with the branch, the branch creates its own `scope`. Protected cleanup may delay completion. `parallel_all` requires a finite positive integer concurrency limit, and the results keep the input order.

`parallel_results` converts the selected expected failures into `Result` and lets the other branches finish. Defects and external interruption remain control events. `parallel_exits` returns opaque full outcomes of the completed branches; external cancellation of the group still propagates. The safe API does not promise to collect all potential errors of a fail-fast operation.

Binding to a scope sets the lifetime; `join` is the wait for a specific result. Structured combinators run their branches in a separate child group with cancellation and join of all active branches, since Effect's `joinAll` stops waiting on an error without interrupting the other fibers.

### Supervised children

`Fiber.spawn owner supervisor task` registers a child with the owner; the task and supervisor cannot capture a shorter region. Losing the Fiber handle does not lose the error: the supervisor receives the full `Failure`, which tells apart the interruption of a closing owner. A defect of the supervisor is passed to the root runtime reporter and fails the run. Long-lived work requires an explicitly longer-lived owner; there is no safe daemon fork with short captures. The signature and the rules are in [Regions and captures, Children](../../reference/regions/#children).

Scoped lifetime and waiting for the natural success of children are different things. An ordinary scoped fiber stops when the scope closes; its error is not attached to the result of the body. A higher-rank callback helps limit the capability of a region, but by itself does not account for children's errors and does not prove the absence of resource escape.

### Race and timeout

`race_success` returns the first success, `race_first` the first completion; losers are interrupted and awaited, and the selected result is preserved. Their Exits are not added to the result, so a cleanup defect of a loser does not become an error of the winner: the `parallel` contract does not carry over to a race.

`timeout` and `timeout_option` use the native choice between the computation and the timer, then interrupt and await the losers. `timeout` adds `TimeoutError`; `timeout_option` represents the timeout itself as `None`, preserving other failures. There is no hard wall-clock guarantee: synchronous JS, an uninterruptible section and cleanup can delay the return, and a timeout never returns while scoped children of the work are still live. It is not guaranteed that all branches start when there is a synchronous winner, nor that the winner is stable when several are ready at once.

**Runtime contract:** native typed recovery, opaque Failure/Exit/RawCause and inert diagnostics; fresh regions/captures; a two-phase scope with mandatory waiting for foreign quiescence; structured combinators and scoped supervision; native race/timeout. Permitted optimizations must preserve these boundaries, the original Cause on rethrow, the service context and the execution of finalizers.
