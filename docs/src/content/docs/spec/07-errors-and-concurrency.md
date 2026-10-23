---
title: "7. Errors, services, resources and concurrency"
---

## Expected errors

Errors have nominal identity and a payload. `try ... with` handles the selected expected error with native Effect semantics. In the first version the payload of every error constructor must satisfy `NoCapture`: it cannot store a scoped handle that captures the region of a callback/task, a cell holding such a value, or hide a capability behind an existential/Unknown. The check applies even to an error that is caught locally. The price of the restriction is that an error carries a path, an id or a copy of the data, not a live resource.

An error is declared with `type error`. Its single constructor has the same name as the error itself and lists the fields like an ADT variant: `type error NotFound = NotFound UserId`. An error without fields is written `type error Timeout`.

```flick
type error NotFound = NotFound UserId

load_optional : UserId ->{Users; none} Option User
let load_optional id =
  try
    Some (Users.get id)
  with
  | NotFound _ -> None
```

Given `Users.get : UserId ->{Users; NotFound} User`, the result of `load_optional` is `Option User`, the requirements remain `{Users}`, and the row of selected recoverable errors becomes empty. The full native Cause may hold additional diagnostic failures; its payloads are not passed off as the residual `e`.

An error can have parameters. `type error RetryExhausted e = RetryExhausted e Number` stores the last cause after the attempts are exhausted, and `type error SendFailed a = SendFailed a` returns the value that could not be sent. The error row distinguishes elements by nominal name `Module.Name`, not by arguments, so a row holds at most one element with a given name: the arguments of two `RetryExhausted` in one row are unified, and `RetryExhausted DbError` together with `RetryExhausted String` is a type error. The TS projection is built the same way: `_tag` does not depend on the arguments, `RetryExhausted<E>` appears in the error union of an `Effect` at most once, and `catchTag` narrows the type unambiguously. Java and OCaml forbid parameterized exceptions for the same reason; here parameters are allowed because the row stores one element per name.

```flick
type error RetryExhausted e = RetryExhausted e Number

retry : Number -> Eff r e a ->{r; RetryExhausted (Errors e)} a
```

Removing an error from a row is allowed only when the selected typed recovery fully covers its nominal variant. A guard or a partial pattern on the payload leaves the error possible. The requirements/errors of the handler are added to the result. Row subtraction applies to the selected recoverable error; it does not prove the absence of such a tag among secondary raw failures.

Expected failure, defect and interruption remain distinct outcomes. An empty error row does not guarantee termination, the absence of a defect, or that cancellation is impossible. In Effect, defects and interruption are kept in the runtime termination cause, not in the ordinary parameter `E`. [Kinds of Effect errors](https://effect.website/docs/v4/error-management/two-error-types).

`try ... with` compiles to handling an Effect typed failure. A JavaScript `try/catch` that surrounds only the construction of an Effect value does not implement this contract.

## Services and type classes

| Mechanism | How the implementation is chosen | What it is for |
| - | - | - |
| Type class | The frontend resolves the instance and passes a dictionary | `Eq`, `Show`, `Functor` and other operations over types |
| Service | The implementation is provided for a particular computation | Database, HTTP, logging, a test environment |
| Layer | The runtime builds services from dependencies | Initialization, resource acquisition and composing the environment |

A pure operation `show user` must not require an Effect Context. Replacing a production service with a test one must not change the global choice of a typeclass instance.

In the direct API, `Users.get` is a generated effectful operation. On the implementation side, a service may store a method that returns an `Effect`. Obtaining a service adds the requirement `Users`; calling an instance already obtained does not have to look it up in the Context again.

Services and errors get the qualified identity of their package and module. A binding of an existing Effect service/error keeps its real key, tag and layout. A service declaration lists the full signatures of the methods; the compiler generates the operations and the type of the implementation record:

```flick
service Clock =
  now : Unit ->{} Number

clock_impl : Clock.Impl
let clock_impl =
  { now = JsClock.now }

clock_layer : Layer none none {Clock}
let clock_layer =
  Layer.succeed Clock clock_impl
```

Here `JsClock.now` is an explicitly declared runtime binding. `Clock.now` requires the service `Clock`; a method of the obtained `Clock.Impl` does not do a repeated lookup. The hidden captures of an implementation record are kept in its type and in the Layer. Service methods do not become class instances. [Effect services](https://effect.website/docs/v4/requirements-management/services).

## Providing the environment

The order of a layer's parameters is `Layer inputs errors outputs`. For example:

```text
UsersLive :
  Layer {Config} {ConnectError} {Users}
```

The block `provide layer { ... }` establishes the environment before the body runs:

```flick
application : UserId ->{Config, Log; NotFound, ConnectError} String
let application id =
  provide UsersLive {
    load_name id
  }
```

If `load_name` requires `{Users, Log}` and allows `{NotFound}`, then after providing `UsersLive` the requirements `{Config, Log}`, the errors `{NotFound, ConnectError}` and the same result `String` remain.

Providing a layer removes the services it supplies, but adds the dependencies and errors of its construction. For an ordinary function, the argument must be `task { ... }` instead of a special block, so that strict evaluation order does not start the body before the environment is established. [Effect Layers](https://effect.website/docs/v4/requirements-management/layers).

## Resources

`scope` combines runtime finalization with a static ban on unsafe escape. Acquisition registers a finalizer with the lexical owner. The safe scoped API has **no early close**; separate manual-handle bindings do not get this guarantee. This restriction makes it possible to start without linearity and without typestate for an open/closed handle.

Fresh regions and transitive tracking of captures are chosen. Through the safe API it is impossible to obtain a usable capability of an already finished region. A native Cause may physically retain a closed foreign object; the safe wrapper does not expose it. A full affine/linear system is not part of the first version. The native [Effect Scope](https://effect.website/docs/v4/resource-management/scope) performs finalization; the static part and the two-phase owner belong to the Flick compiler/runtime adapter.

A scoped finalizer has an empty expected-error channel; its failure remains a defect in the full outcome. The lexical `bracket` allows a separate typed release error and adds it to the error row. The two operations are explicitly distinct. The primary error and cleanup are preserved under the native Cause contract; release is not replaced by an ordinary JS `finally` that loses the original failure.

## Concurrency

The main API is the structured `parallel` and `parallel_all` over cold `Eff`s. They cancel the active branches on failure and wait for them to finish. `parallel_all` requires a finite positive integer concurrency limit; success results follow the input order.

Several observed failures are kept in the native Cause; the API does not promise to collect all future errors. `parallel_results` separately accumulates the selected typed errors as `Result`, and `parallel_exits` accumulates opaque full outcomes. Defects and external interruption are not masked by an ordinary successful list.

`spawn` requires an explicit lexical owner and supervisor; `join` waits for a specific fiber and restores its failure. Child errors are not added to the parent's error row automatically, so a supervisor is mandatory even if the handle may be ignored. The safe API has no unbounded daemon fork with local captures. [Effect Fibers](https://effect.website/docs/v4/concurrency/fibers).

## The boundary of the language core

Retry, schedules, streams, queues, cache, tracing and HTTP remain a typed standard library. For re-running and concurrency it accepts descriptions of computations. There are no special syntax constructs for them in the first version.

A full `deriving Schema` and codecs for external formats are part of the target language. Generics, recursion, refinements, field policies and effectful transformations are covered in section 9.

Services and `provide` do not mean universal algebraic handlers with capture and multi-shot resumption of a continuation. In the first slice, the language's operations rely on concrete Effect mechanisms; universal handlers are not part of this contract.

## Execution and static lifetime contracts

The basic rules below describe program behavior that must remain the same when the lowering changes. More complex modes can be added as separate library operations.

| Area | First-version contract | Price |
| - | - | - |
| Typed errors | Nominal, region-free payloads; `e` is the selected recoverable error | Not every raw Fail can be treated as a value of `Errors e` |
| Defects and interruption | Kept outside `e`; available through opaque Failure/Exit | An ordinary handler is not a universal catch |
| Provide | Layer construction first, then the body; explicit override of conflicts | No automatic choice among ambiguous services |
| Sharing | One Layer identity is shared within one build | Explicit owned memoization is needed between provides |
| Scope | Fresh regions/captures; children finish first, then LIFO resource cleanup | Closing may wait for uninterruptible or foreign work |
| Cleanup errors | Scoped finalizer without an expected error; typed release via bracket | Errors of all dynamic finalizers are not merged into e |
| Parallel | Fail-fast, cancellation of active branches, waiting for cleanup, input order of results | Does not guarantee all potential failures |
| Spawn | Explicit owner and mandatory supervisor; join restores the failure | The parent's success does not imply the success of background tasks |
| Race/timeout | Native choice of the winner and waiting for losers | No hard deadline and no automatic addition of loser failures |

## Error as data and error as an execution failure

A pure function can return a `Result` without running an Effect. The source order `Result error value` is convenient for HKT; the runtime representation is the native `Result.Result<A, E>` from [Effect v4](https://effect.website/docs/v4/data-types/result), with the parameters swapped in the TS projection. The parameter `error` here has kind `Type`; the row of runtime errors has kind `ErrorRow`. These are different positions.

```text
parse_config :
  String -> Result ParseError Config

fetch_config :
  Url ->{Http; NetworkError} Config
```

Converting a `Result` to a typed failure is an explicit library operation:

```text
from_result : Result (Errors e) a ->{; e} a
```

For `Result NotFound User` the singleton row `{NotFound}` is used. `Result String a` requires first converting the string to a declared nominal error. The operation does not catch defects or interruption. An ordinary `try ... with` handles expected failures; the separate `Exit`/`Cause` API handles the full outcome. [Separation of Effect errors](https://effect.website/docs/v4/error-management/two-error-types).

```flick
type error NotFound = NotFound UserId
type error DbUnavailable = DbUnavailable String

find_user :
  UserId ->{Users; NotFound, DbUnavailable} User

find_optional :
  UserId ->{Users; DbUnavailable} Option User
let find_optional id =
  try
    Some (find_user id)
  with
  | NotFound _ -> None
```

### A complex Cause: what the error row promises

In the accepted contract, `e` describes **the error available to ordinary typed recovery**. The full native Cause may contain secondary payloads outside the residual `e`; the safe API does not provide `Failure e -> List (Errors e)` for all of its Fails.

The verified reference is Effect 4.0.5, commit `81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f`. In v4, a Cause holds a flat list of `Fail`, `Die`, `Interrupt`; a tree of sequential/parallel causes is not part of the contract. [Cause](https://effect.website/docs/v4/data-types/cause).

| Cause | Native typed recovery in the verified slice |
| - | - |
| A single `Fail(A)` | A is selected |
| `Fail(A), Fail(B)` | The first Fail is selected; the handler does not walk the whole list |
| `Fail(A), Interrupt` | The Interrupt itself in the list does not block selecting A |
| `Fail(A), Die(D)` | Ordinary typed recovery is blocked; the Fails are turned into defects |
| Only Interrupt | An ordinary typed handler receives no error |
| `Fail(B), Fail(A)`, handler for A only | The handler is not called; the original cause propagates |

The last row is significant: the native `catchTag(A)` reduces E in the TS signature, although a secondary A may remain in the full Cause. So it is impossible both to inherit this handler mechanically and to pass every raw `Fail` off as `Errors e`. The [selection of the recoverable error](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/internal/effect.ts#L160-L199) and [catchIf/catchTag](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/internal/effect.ts#L2947-L3068) were verified.

### The chosen Cause contract and the price of the alternatives

| Model | Guarantee | Price |
| - | - | - |
| Native recovery and opaque RawCause — chosen | The handler corresponds to the selected recoverable error; rethrow preserves the native Cause | Safe diagnostics return an inert report, not the original JS payloads |
| Normalizing the residual cause — outside the first version | All active Fails belong to the residual e; secondary excluded failures are stored separately | A custom adapter for every narrowing handler, walking the causes, and a difference from native catchCause |
| A fully typed Cause — outside the first version | All Fails have type Errors e | A partial handler often cannot reduce e; additional proofs are needed |

`Exit e a`, `Failure e` and `RawCause` are opaque types. Inside, the native Effect representation is kept; the typed and diagnostic interfaces are separate.

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

A successful `Exit` holds `a` and preserves the captures of that `a`. `Failure.primary` yields only the selected typed error; `rethrow` restores the original Cause with the same `e` or with its permitted extension. There is no safe `RawCause -> Exit e a` for an arbitrary `e`. The operations cannot erase known captures. Typed error payloads are checked to be region-free; arbitrary foreign defects remain sealed.

A safe `RawCause` **does not expose the original `JsValue`**. `CauseReport` holds inert kinds of causes, safely obtained tags/text and interruption metadata. Inspection does not call user getters, `toString` or `toJSON`; an unsupported payload is shown as a placeholder. Access to the original JS graph is possible only through an explicit unsafe FFI. Bindings of native Cause transformers are verified separately: it is impossible to attribute the residual `e` to every secondary Fail or to run a callback for it under a false type.

In the second variant `[Fail(B), Fail(A)]`, after handler A the active one becomes `Fail(B)`, with the original A kept in the diagnostics. Turning this A into a `Die` is not allowed: it would change the subsequent typed recovery. In the third variant, the rule "we simply skip the complex cause, but still subtract A" is also incorrect.

An additional variant for `parallel` is an explicit `CompositeError`. It changes the application API: handling `NotFound` is not the same as handling an aggregate with a NotFound inside. Such a type is useful as a separate combinator, but is not required for all computations.

A successful recovery may absorb the cause and its secondary diagnostics. If they are needed in logs, observation is done before the recovery. The rule for selecting an error from the list does not mean that a handler may arbitrarily disable an already requested fiber cancellation: the interruption state belongs to the runtime.

## Lifetime and sharing under provide

`provide` first builds the Layer completely and only then runs the body. If construction fails, the body does not start. Scoped implementations belong to the lexical owner of the provide; on exit the outer environment is restored and a two-phase close is performed. The captures of a service, implementation record, closure, `Eff`, UI or codec are preserved in interfaces and cannot outlive the owner.

`task { service_call () }` looks up the required service at run time in the current environment. If the task has already captured a specific instance of the service, it keeps that instance's lifetime. A finalizer receives the context saved at registration, not an arbitrary service from the environment of a later close.

One Layer identity is reused within one graph build. Independent `provide`s build independent instances; there is no global singleton. Long-lived shared construction requires explicit memoization with an owner. Two implementations of one service key in a graph are an error, except for an explicit override combinator. This rule relies on [Effect layer memoization](https://effect.website/docs/v4/requirements-management/layer-memoization), and the source checker additionally verifies lifetime.

## Finalization and resource boundaries

```flick
read_config :
  Path ->{FileSystem; OpenError, ReadError} String
let read_config path =
  scope {
    let file = File.open_scoped path
    File.read_all file
  }
```

`File.open_scoped` requires a lexical `Scope s`, which `scope` provides. After a successful acquisition a finalizer is registered. The result is returned after the children owned by the region finish and the resource cleanup that follows. Resource finalizers run LIFO. A single mixed LIFO list of children and resources does not ensure this if a resource is registered after a child has started. [Scope API](https://effect.website/docs/v4/api/effect/Scope).

A scoped `acquireRelease` requires a release without expected errors; the lexical `bracket`, built on `acquireUseRelease`, allows a typed release error. This is exactly the separation adopted in the language. Cleanup defects are preserved in the full outcome; the successful closing of every external resource is not a theorem of the checker. [Native Effect operations](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/Effect.ts).

For safe scoped APIs the checker verifies direct and indirect escape: the result, closures, task, containers, existential wrappers, writes to external state and handing over to a retaining FFI. A resource can be used many times inside its region if its API allows it. Static lifetime by itself does not prove the absence of races or the safety of concurrent use of a handle.

### Static regions and captures

Every `scope s { ... }` creates a fresh region `s` and a runtime owner. In `scope { ... }` the name is inferred and stays hidden. A scoped handle has a type such as `File s`. The owner is passed by the compiler lexically; it is not implemented by a single ordinary Context lookup, which in a nested region could pick a different scope.

```flick
bad_deferred :
  Path ->{FileSystem; OpenError} Eff none {ReadError} String
let bad_deferred path =
  scope {
    let file = File.open_scoped path
    task { File.read_all file }
  }
```

This is an error: the task captures `file`, yet it is being returned after the region has closed. The ordinary type `Eff none {ReadError} String` does not show this dependency, so a check of `s` in the result type alone is not enough.

The correct fix is to move the acquisition inside the deferred execution:

```flick
deferred_read :
  Path -> Eff {FileSystem} {OpenError, ReadError} String
let deferred_read path =
  task {
    scope {
      let file = File.open_scoped path
      File.read_all file
    }
  }
```

Now every `await` gets its own region and resource. The returned string may outlive the file.

The same check is needed for an ordinary callback:

```flick
bad_callback :
  Path ->{FileSystem; OpenError} (Unit ->{; ReadError} String)
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
  File s -> (Eff none {ReadError} String)^{s}

scoped_callback :
  forall s.
  File s -> (Unit ->{; ReadError} String)^{s}
```

These are interface signatures. Local captures are inferred; public signatures and `.iface` preserve them. Captures pass through partial application, records, opaque/existential types, dictionaries and `Unknown^{s}`. Passing a callback through a generic higher-order API requires a capture-polymorphic contract, like `List.map_effect`. In short scoped examples the region name is omitted in the syntax, but the checker keeps it. Mutable storage is invariant; a longer-lived Ref/cache cannot receive a shorter capability.

Capture checking in [Scala](https://docs.scala-lang.org/scala3/reference/experimental/cc.html) gives a useful reference for closures and containers; it is not a ready-made implementation of the Flick checker.

### Scope of the strong contract

| Property | First-version contract |
| - | - |
| Using a handle many times in a live region | Allowed if the resource's API permits it |
| Early close | Absent from the safe scoped API |
| Full linearity | Not introduced; lifetime is checked through regions/captures |
| Concurrent access | The binding declares share-safe operations or serializes access; lifetime does not prove the absence of races |
| Cleanup | Effect runs the finalizers; the compiler does not prove the physical success of an external close |
| Typed errors | The payload is region-free even under local recovery |
| Native Cause | Live foreign payloads are sealed; only an inert report is safely accessible |
| Unchecked JS | Requires an explicit trusted/unsafe contract |

### Fiber owner and release order

A task can capture resources of its own region and of longer-lived regions. Moving to a longer-lived owner is allowed only if there are no short captures. A subscription or callback retained by an external library also has a verifiable owner.

Closing a strong scope has a mandatory order: **forbid new work → request cancellation of the children → wait for the terminal state of each → run resource finalizers LIFO → return the final Exit**. If a child does not finish an uninterruptible section, the close waits. Releasing a resource that is in use under a child that is still running is forbidden; a timeout does not override this rule.

A terminal state of an Effect fiber does not yet prove that an external Promise operation has stopped. An async FFI with scoped captures must, before completing cancellation, wait for confirmation of the stop or for the actual end of the external work. A lone `AbortSignal` is not enough. If an operation cannot be cancelled, the adapter waits for it to end; detach is allowed only without short captures or with an explicit safe transfer of ownership.

Simply mixing `forkScoped` and resource finalizers in one LIFO list does not guarantee this order for every registration. The backend must have a separate group of children or an equivalent split into scopes. This is an adapter on top of Effect.

A nursery is not part of the first version. Structured combinators own their branches; a scoped background fiber is cancelled when the owner closes. Errors of background work are always observed by the explicitly passed supervisor, and `join` additionally restores the failure for the waiter. This does not mean automatically merging child errors into the error row of the scope body.

## Concurrent operations with different contracts

```flick
load_pair : UserId -> UserId ->{Users; NotFound} (User, User)
let load_pair first_id second_id =
  parallel
    (task { Users.get first_id })
    (task { Users.get second_id })
```

For `parallel` the order of results matches the order of arguments. A failure stops the launch of further work, requests cancellation of the other active branches and waits for them to finish. In the verified Effect implementation the observed non-Interrupt causes are collected, including some failures of the branches being stopped; receiving all possible future errors is not promised.

The operation does not close an external `Scope`: resources registered in it live until it is closed. If a resource must close together with the branch, the branch creates its own `scope`. Protected cleanup may delay completion. `parallel_all` requires a finite positive integer concurrency limit, and the results keep the input order. [Effect's concurrent traversal implementation](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/internal/effect.ts#L5098-L5252).

`parallel_results` converts the selected expected failures into `Result` and lets the other branches finish. Defects and external interruption remain control events. `parallel_exits` returns opaque full outcomes of the completed branches; external cancellation of the group still propagates. The safe API does not promise to collect all potential errors of a fail-fast operation.

Binding to a scope sets the lifetime; `join` is the wait for a specific result. In the current [Fiber API](https://effect.website/docs/v4/api/effect/Fiber#joinAll), `joinAll` stops waiting on an error but does not itself interrupt the other fibers. So the backend implements structured combinators through a separate child group with cancellation and join of all active branches, not through a single call to joinAll.

### Concurrency: three models

| Model | Completion and errors | Price |
| - | - | - |
| Structured combinators + explicit scoped fibers — chosen | `parallel` owns the branches; background tasks are started by a separate operation, the result is received through the mandatory supervisor and, if needed, join | Dynamic groups require passing group control explicitly |
| Language nursery | A normal exit waits for the children; an error of the body/child cancels the related participants; child errors are accounted for even without join | A registry of children, rules for merging causes, and accounting for the children's error row in the nursery's type |
| Scoped background supervision | Closing the scope cancels background tasks; errors go to a supervisor, log or channel | The parent's success does not imply the success of all background tasks; an explicit observation policy is needed |

The first version uses structured combinators and scoped supervision. `spawn owner supervisor task` registers a child with the owner; the task and supervisor cannot capture a shorter region. Losing the Fiber handle does not lose the error: the supervisor receives the full `Failure`, distinguishing expected shutdown interruption. A defect of the supervisor is passed to the root runtime reporter. For UI, a rule additionally applies that the root terminates when its supervisor fails. Long-lived work requires an explicitly longer-lived owner; there is no safe daemon fork with short captures.

Scoped lifetime and waiting for the natural success of children are different things. An ordinary scoped fiber stops when the scope closes; the error is not automatically attached to the result of the body. In a nursery such attachment is an additionally promised contract. A higher-rank callback helps limit the capability of a region, but by itself does not account for children's errors and does not prove the absence of resource escape.

### Race and timeout

| Contract | Behavior | Price |
| - | - | - |
| Native operations — chosen | `race_success`: the first success; `race_first`: the first completion; losers are interrupted and awaited; the selected result is preserved | Their Exits are not added to the result automatically; a cleanup defect of a loser may not become an error of the winner |
| `race_checked` outside the first version | After the choice, the result is checked against the stop outcomes of the losers | The winner may be rejected; early failures must be told apart from cleanup failures |
| Early timeout outside the first version | The response is returned after the timer is selected, and completion of the work is handed to a longer-lived owner | The work lives on after the response; a scope, a supervisor and rules for subsequent errors are mandatory |

In the verified [Effect race](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/internal/effect.ts#L1649-L1739), waiting for the losers does not add their Exit to the selected result. So the `parallel` contract cannot be carried over to a race automatically.

`timeout` and `timeout_option` use the native choice between the computation and the timer, then interrupt and await the losers. `timeout` adds `TimeoutError`; `timeout_option` represents the timeout itself as `None`, preserving other failures. There is no hard wall-clock guarantee: synchronous JS, an uninterruptible section and cleanup can delay the return. `race_checked` and an early return with still-live scoped children are not part of the first version. It is not guaranteed that all branches start when there is a synchronous winner, nor that the winner is stable when several are ready at once.

**First-version runtime:** native typed recovery, opaque Failure/Exit/RawCause and inert diagnostics; fresh regions/captures; a two-phase scope with mandatory waiting for foreign quiescence; structured combinators and scoped supervision; native race/timeout. Permitted optimizations must preserve these boundaries, the original Cause on rethrow, the service context and the execution of finalizers.
