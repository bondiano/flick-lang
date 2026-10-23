---
title: "8. TypeScript generation and the FFI boundary"
---

## Pure functions

```flick
identity : a -> a
let identity value =
  value

greet : String -> String -> String
let greet prefix name =
  prefix ++ ", " ++ name

hello : String -> String
let hello =
  greet "Hello"
```

```ts
export const identity = <A>(value: A): A =>
  value

export const greet =
  (prefix: string) =>
  (name: string): string =>
    prefix + ", " + name

const hello = greet("Hello")
```

Pure functions get an ordinary JS/TS representation. Currying is first kept literally; the optimization of full application must not change the order or the moment of argument evaluation.

## ADTs and pattern matching

```flick
with_default : a -> Option a -> a
let with_default fallback value =
  match value with
  | None -> fallback
  | Some x -> x
```

```ts
import { Option } from "effect"

const withDefault =
  <A>(fallback: A) =>
  (option: Option.Option<A>): A => {
    switch (option._tag) {
      case "None":
        return fallback
      case "Some":
        return option.value
    }
  }
```

In this example `Option` is a standard type with a native Effect representation. A user-defined ADT with a similar name or tags remains a different type. The source frontend ensures exhaustiveness and nominal identity; `readonly` in TS describes the external interface, while the immutability guarantee also depends on the FFI and the available primitives.

### Variants, guards and or-patterns

A constructor carries zero, one or several fields. The fields are listed after the constructor name in the same way as the arguments when it is applied and in a pattern: `| Circle Point Number`. A field is an atomic type, so a type application and an arrow are parenthesized: `| Branch (List (Tree a))`, `| Handler (a -> b)`. `Circle (Point, Number)` is one tuple field, not two fields. In the ABI the fields occupy `_0`, `_1` and so on in declaration order.

The right-hand side of `type` always lists variants, and the leading `|` is optional: `type Wrapper = Wrap Number` is an ADT with a single constructor. A transparent alias, including a record type, is declared with `type alias`: `type alias UserId = Number`. `opaque type alias` hides the representation outside the module, and `opaque type` with variants hides the constructors. `alias` is a keyword only immediately after `type`.

```flick
type Shape =
  | Circle Point Number
  | Rect Number Number
  | Empty

area : Shape -> Number
let area shape =
  match shape with
  | Circle _ radius -> Number.pi * radius ** 2
  | Rect width height when width == height -> width ** 2
  | Rect width height -> width * height
  | Empty -> 0

is_round : Shape -> Bool
let is_round shape =
  match shape with
  | Circle _ _ | Empty -> true
  | Rect _ _ -> false
```

A `match` or `try` branch may have a guard `when e`, and a pattern may have alternatives `p | q`. The restrictions follow Rust:

| Rule | Where it is checked |
| - | - |
| All alternatives `p \| q` bind the same names of the same types | Names — parser (`FLICK-S012`), types — checker |
| A name is bound at most once by one pattern or by the parameters of one function | Parser (`FLICK-S012`) |
| An or-pattern is allowed in a `match`/`try` branch and nested: in parentheses, record fields and List elements | Grammar |
| A guard applies to the whole branch, including all its alternatives, sees the names of its pattern and binds no new ones | Grammar |
| A guard is a pure expression of type `Bool`; a runtime call in a guard is forbidden, otherwise the order of checking branches would become observable | Checker |
| A branch with a guard does not count in the exhaustiveness proof | Checker |

Branches are checked top to bottom, alternatives left to right; the first matching branch whose guard is true is executed.

## An effectful function

```flick
load_name : UserId ->{Users, Log; NotFound} String
let load_name id =
  let user = Users.get id
  let name = normalize_name user.name

  Log.info name

  name
```

```ts
export const loadName = (
  id: UserId
): Effect.Effect<string, NotFound, Users | Log> =>
  Effect.gen(function* () {
    const user = yield* getUser(id)
    const name = normalizeName(user.name)

    yield* logInfo(name)

    return name
  })
```

`getUser` and `logInfo` here are generated accessors. The backend inserts `yield*` on effectful calls and leaves pure calls ordinary. Generators are one way of composing Effect, not a requirement on the source syntax. [Effect.gen](https://effect.website/docs/v4/getting-started/using-generators).

## HKT at the TypeScript boundary

HKTs and source kinds are checked by the frontend; when JS is generated they are erased or remain in the dictionaries that are passed around. The public TS API of the first version exports concrete applications of HKTs and representable dictionary records. An abstract higher-kinded export that TS cannot express correctly requires an explicit concrete façade; the importer/exporter does not pretend to express it through a false `F<A>` or `any`.

The native notation `F<A>` for an arbitrary type parameter `F` is not a TypeScript feature. A successful check of the TS projection therefore does not replace checking the source program. [TypeScript for Functional Programmers](https://www.typescriptlang.org/docs/handbook/typescript-in-5-minutes-func.html).

## FFI

| Category | Contract |
| - | - |
| Pure binding | An explicit trusted assertion about the purity of the call and of module initialization |
| Sync operation | Deferred until run; exceptions are translated only by an explicit mapping |
| Async operation | The Promise is created at run time; the policy determines cancellation and waiting for quiescence |
| Effect-producing function | `foreign effect` defers the call of the JS factory and runs its result in the current fiber |
| Ready-made Effect value | Imported as a cold Eff; effectful module loading remains runtime |
| External data | Arbitrary input is JsValue; source records/ADTs require a checked adapter |

Creating a Promise ahead of time breaks the coldness of `task`. If external work uses a scoped capability, on cancellation the adapter must wait for the confirmed stop or completion of the underlying operation. Neither the `AbortSignal` itself nor the completion of the interrupted fiber proves this. An uncancellable operation keeps its owner until it finishes. It may be detached only if there are no short-lived captures, or after an explicit safe transfer of ownership.

The main path is explicit bindings to Effect and JS APIs. The target plan also includes an importer for a supported subset of `.d.ts`, which generates the same bindings, shows the gaps explicitly and allows local corrections to the contract. Connecting npm does not require waiting for a complete port of the TS type system.

The first-version codegen turns direct self tail recursion of a pure function into a loop, preserving the order of argument evaluation. Ordinary non-tail and mutual pure recursion get no general stack-safety guarantee; an explicit loop/trampoline API is available for them. Recursive runtime execution is built through suspend/continuation, without eagerly creating the whole Effect chain recursively. Pure loops do not acquire automatic cancellation checkpoints; turning an unbounded sequence of runtime steps into such a loop is forbidden.

## Direct FFI and the cost of automation

The main path is chosen: direct typed bindings to JavaScript, a fixed data ABI and a limited TS projection. A generator from `.d.ts` complements this path. The approach with an explicit type of the foreign function and a link to the JS export is close to the [PureScript FFI](https://book.purescript.org/chapter10.html); [Elm ports](https://guide.elm-lang.org/interop/ports.html) remain an example of a separate message-passing architecture.

| Approach | Gain | Cost |
| - | - | - |
| Message ports | An explicit boundary of state ownership and messages | Every small call requires a protocol; it is hard to use JS APIs freely |
| Explicit foreign bindings | A predictable link between types, effects and the JS call | The binding author writes and checks the contract |
| Generating bindings from supported `.d.ts` | Less manual work for typical libraries | Does not infer purity, runtime validation, cancellation and data ownership |
| Importing the full expressiveness of TS | Maximum npm coverage | Requires a complex reconciliation of two type systems; conditional types, overloads and HKTs are not carried over automatically |

The `.d.ts` importer builds explicit bindings, uses TypeScript to resolve declarations and grows the supported subset package by package. An unsupported export remains a named blocked item with a reason; the correct independent exports of the package are available. Unchecked types are not replaced with `any` or pure source types. The generated file and the reviewed overrides are kept separate, so that a package update does not erase accepted contracts.

### The supported d.ts subset

| First-version surface | Import result | Explicit contract |
| - | - | - |
| Primitives, literals, tuples, arrays, object shapes, opaque foreign classes | A representable source type or a foreign handle | ABI, mutability/ownership and validation |
| Optional/nullable and discriminated unions | Only an unambiguous mapping to the chosen ABI | The distinction between missing/null/undefined and the constructor tags |
| Ordinary rank-1 generics | Supported if parameterization and representation are preserved | `.d.ts` does not prove parametricity |
| Finite unambiguous overloads | Separately named wrappers | Unresolvable ambiguity blocks the export |
| Concrete mapped/conditional/indexed-access types | Only after TS fully reduces them to a supported form | Dependent unreduced type operators are not supported |
| Promise and callbacks | The binding shape is generated; activation requires an adapter policy | Errors, owner, retention, cancellation/quiescence, supervisor |
| `any` and TS `unknown` | `JsValue` | They do not become a trusted source Unknown |
| HKTs, source classes, regions/captures | A native `.iface` or a concrete façade | A bare `.d.ts` is not a complete source interface |

For each selected export the importer gives one of three statuses: supported; needs policy/adapter; unsupported. Supported independent exports are available even if another export is blocked. A simple TS object shape stays by default a foreign shape with runtime access until a checked snapshot/adapter; it does not become an immutable source record because of `readonly`. Coverage grows with importer versions and does not change the meaning of an already reviewed binding.

### An example of an imported function

A hypothetical library declares:

```ts
export declare function lookup(
  id: string
): Promise<string | undefined>;
```

With the explicitly chosen policy `undefined -> None` and rejection `-> JsError`, the importer may produce:

```flick
lookup : String ->{; JsError} Option String
foreign async lookup from "package" "lookup"
  arity 1
  catch JsError.from_js
  nullable
```

The conversion of the result is set by the metadata/adapter binding: `Option` in the ABI is a native Effect value, so `undefined -> None` is written with the line `nullable` rather than inferred from the result type. The importer reads package policies from `policy.toml` next to the generated module (`undefined = "none"`, `catch = "JsError"`, `retention`, `cancel`, `mode = "pure"`, `strict`) and never writes it. A blocked export is generated as `foreign blocked name "reason"`: using it is a compile error, and the other exports of the package are available. A manually reviewed binding carries the metadata `reviewed "<pkg>@<ver> sha256:…"` — the fingerprint of the declaration against which it was checked; `ffi check` compares it with the installed version. The Promise is created when the Effect runs, not when the task is built. This signature contains no proof of cancellation; a signal is added only for a JS API that really supports it.

Another export:

```ts
export declare function nextId(): number;
```

conservatively gets `Unit ->{} Number`. The function may read the clock or change a counter. A pure arrow requires a reviewed contract; it cannot be inferred from the return type alone.

### Diagnostics and package updates

The importer produces a report for the selected package surface and a named reason for every blocked export. `any` and TS `unknown` map to `JsValue`, or are rejected by the package's strict mode; there is no silent weakening of a source type. Overrides are stored separately from the generated file and are rechecked after a dependency update.

The contract stores the origin of the declaration and the dependency version; an update rechecks the shape and the compatibility of the overrides. Overloads can be imported as separately named bindings if a safe automatic resolution is not defined.

A callback with unknown retention does not accept a scoped handle or a closure capturing it. A "for the duration of the call only" agreement, a scope-owned registration, or an explicit unsafe boundary is required. TS `readonly` does not rule out changes through another alias; this is discussed directly in [TypeScript object types](https://www.typescriptlang.org/docs/handbook/2/objects.html).

[ReScript FFI](https://rescript-lang.org/docs/manual/bind-to-js-function/) is useful as an example of a declarative JS ABI: methods, `this`, the number of arguments and variadic calls. Its approaches can be used independently of the chosen source type system.

The Content Mapper links the sources and the TS projection. Support for an imported type, region checking and the correctness of the runtime adapter are ensured by the frontend; growing the Mapper's capabilities does not by itself carry TS semantics over into Flick.

## One signature and an explicit mode of the foreign call

The modes `foreign pure`, `foreign sync`, `foreign async`, `foreign effect` are chosen. The full source signature stands above the declaration; `arity`, `this`, `catch`, the nullable mapping, retention and cancellation are set by binding metadata. The names `cancel signal` and the position of the signal determine how the argument is passed, but do not replace the quiescence contract.

```flick
hypot : Number -> Number -> Number
foreign pure hypot from global "Math.hypot"
  arity 2

now_millis : Unit ->{} Number
foreign sync now_millis from global "Date.now"
  arity 0

read_text : Path ->{; FsError} String
foreign async read_text from "./fs.js" "readText"
  arity 1
  catch FsError.from_js
  cancel signal

fetch_user : UserId ->{Http; HttpError} User
foreign effect fetch_user from "./users.js" "fetchUser"
  arity 1
```

`arity` sets the number of ordinary positional arguments of the JS call. `arity 2` adapts two stages of currying; `arity 0` takes the source `()` and calls JS without arguments. The pair `(Number, Number)` is not unpacked into two arguments by itself. `cancel signal` separately adds a signal as the binding specifies, in this example as the last argument.

The source is `from global "Math.hypot"` for a global JS value or `from "./fs.js" "readText"` for an export of an ESM module. Metadata are indented lines under the declaration, one item per line: a name and atom arguments. The known names are: `arity N`, `this`, `catch f`, `cancel signal`, `nullable …`, `retention …`, `optional …`, `variadic …`; an unknown name is the error `FLICK-S011`. The parser checks only the "name and arguments" form; whether the arguments of each item are admissible for the given signature and mode is checked by the checker's FFI check.

Removing the physical `Unit` argument does not remove the computation that produced it: `now_millis (Log.info message)` first performs the logging, in source order.

The JS adapter `readText` takes a path and an `AbortSignal`, and `FsError.from_js` explicitly translates a failure into a region-free error. The Promise is created only when run at runtime. Below is an admissible lowering for an operation that has no scoped handle passed in; it does not serve as a universal implementation of safe async FFI with a captured resource:

```ts
export const readText = (
  path: string
): Effect.Effect<string, FsError> =>
  Effect.tryPromise({
    try: (signal) => readTextJs(path, signal),
    catch: toFsError
  })
```

`foreign effect` must defer the call of the whole JS factory: the TS type of a function with an Effect result does not prove that building it is pure.

```ts
const fetchUser = (id: UserId) =>
  Effect.suspend(() => fetchUserJs(id))
```

The call is executed at run time in the current fiber. A throw from the factory becomes a defect unless the binding explicitly translates it into a typed error. A separate trusted `foreign pure` may build an `Eff` purely, but requires a stronger contract. These source declarations differ even with the same TS type. [Effect.suspend](https://effect.website/docs/v4/getting-started/creating-effects).

A throw and a rejected Promise are translated into a typed error only by the declared mapping; otherwise they remain defects. A mapping cannot put a scoped capability into an error payload. For scoped arguments a retention and quiescence contract is mandatory. An import/initialization error of an external module also happens at runtime and is handled by its binding. [Creating Effects](https://effect.website/docs/v4/getting-started/creating-effects).

## Value representation and callbacks

| Source value | First-version ABI |
| - | - |
| Unit | JS `undefined`; a successful foreign void is normalized to `()` |
| Number, BigInt, Bool, String | The corresponding JS primitives |
| Tuple | A dense JS array of fixed length |
| List | A dense JS array with an immutable source API |
| Record | A plain JS object with the exact set of own data fields |
| User-defined ADT | An object with `_tag` = `Name#hash`, where hash is the high 48 bits of the 64-bit FNV-1a of the qualified constructor id; the payload is in `_0`, `_1` and so on |
| Nominal wrapper | A real tagged object by default; `unboxed type` declares the representation as a field, `untagged type` as fields distinguishable by JS type |
| Native Effect types | The native ABI of the pinned version; real constructors/protocols |
| Unknown input from JS | An opaque JsValue until a checked runtime import |
| Nullable/optional | An explicit adapter distinguishes null, undefined and a missing key |
| Methods | `this`, arity, evaluation order and listener identity are preserved |
| Callbacks | Four explicit families: pure, Effect, Promise, scoped event |

The FFI supports fixed uncurried calls, `this`, optional arguments and a variadic tail through an explicit List adapter. Flick's currying is preserved. The constructor id includes the nominal identity of the package/module/type/constructor and is recorded in `.iface` (the field `id` next to `tag`); `_tag` carries the constructor name and the hash of this id, which is shorter than the full id in the bundle and does not reduce identity to a single name. Immutable data is copied by default at an untrusted input and output boundary. Opaque native handles are passed only to their own adapter, without an attempt to copy mutable internals. [ReScript bindings](https://rescript-lang.org/docs/manual/bind-to-js-function/) give examples of such calling conventions.

For JS callbacks **four explicit families of adapters** are chosen. An additional sync modality and the automatic conversion of a `void` callback into running an Effect are not part of the first version. A runtime arrow with empty rows does not prove synchrony.

| Family | What JS gets | Who controls execution | Where to use |
| - | - | - | - |
| Pure callback | An ordinary result value | A direct call | Comparator, `map`, predicate, synchronous transformation |
| Effect callback | A lazy `Effect<A, E, R>` value | The receiving Effect API | Native Effect combinators; the current fiber and environment are preserved |
| Promise callback | `Promise<A>` | The explicitly provided runtime and adapter | An API that really awaits a Promise |
| Scoped event callback | Usually `void`; the fiber/queue belongs to the subscription | The Scope owner and the supervisor | DOM events, WebSocket, event emitter, external notifications |

### The cost of the chosen callback ABI

| Approach | Benefit | Cost |
| - | - | - |
| Four explicit families — chosen | The contract is visible in the binding; managed errors, cancellation and lifetime | More adapter declarations; an arbitrary arrow cannot be passed to an arbitrary callback slot |
| A sync modality outside the first version | Limited synchronous actions with an immediate result could be allowed safely | `r/e` alone are not enough: proofs of the absence of suspension and FFI rules are needed; a synchronous action does not become pure |
| An automatic bridge outside the first version | Short source code for simple integrations | `void` does not say whether JS observes the Promise, waits for it and who cancels the work; trusted annotations are needed and all the same runtime contracts |

`Effect.runSync` is not a universal adapter for `->{}`: at an async boundary it throws `AsyncFiberError`, and the started computation may keep running. This behavior is described directly in [Running Effects](https://effect.website/docs/v4/getting-started/running-effects).

### Subscriptions and event handlers

```flick
on_message : Message ->{Log; none} Unit
let on_message message =
  Log.write message.text

start_listening : Unit ->{Scope, Log; none} Unit
let start_listening () =
  Events.subscribe_messages
    (Events.queue 64)
    Events.log_failures
    (fun message -> task { on_message message })
```

Here `Events.queue 64` sets a bounded FIFO policy, `Events.log_failures` is an explicit standard-library supervisor, and the callback creates a cold task. Overflow rejects the new event and reports it to the supervisor, without growing the queue indefinitely. The adapter preserves the environment and the listener identity, allows a reentrant call during registration only after the owner/queue is initialized, synchronously forbids late callbacks on close, removes the listener, interrupts and joins the tasks, and only then releases the captures. A snapshot of the message is taken before it is passed into the fiber. Creating a runtime outside the application is possible only through an explicit boundary adapter.

A late callback after closing has begun does not start a user task. Re-registration creates a separate owner/queue; exactly the registered JS function is removed. The binding does not turn events into independent forgotten `runPromise` calls.

If JS requires an immediate action — for example, a decision about the default action of an event — the asynchronous part cannot promise it in time. A binding in two parts is possible: a pure callback immediately returns the decision, the adapter performs the required synchronous action, and then starts the scoped processing.

### Higher-rank callbacks at the FFI

`(forall a. a -> a)` is exported in the supported subset as a TS callback `<A>(value: A) => A`. The polymorphism belongs to the callback itself, not only to the outer function. HKTs and class constraints require an explicit encoding/dictionary ABI or a concrete façade; a `.d.ts` type does not prove the parametricity of an unknown implementation.

For all families the binding preserves the JS arity, `this`, the handler identity for removal, and a consistent conversion of exceptions. The Promise boundary separately defines the mapping of typed errors, defects and interruption; cancellation requires a real protocol, such as `AbortSignal`, and does not arise from the Promise type itself.

Direct FFI and data validation solve different problems. Passing a trusted `HTMLElement` as an opaque handle does not require a JSON codec. HTTP data, configuration or an unknown JS object require validation before being turned into a `User`. A `.d.ts` signature by itself does not check a value and does not prove the absence of mutation.

The separation of **`Unknown` and `JsValue`** is chosen. `Unknown` is an unknown source value with preserved capture provenance: the part available for structural traversal is inert and immutable, and trusted opaque leaves remain opaque. `JsValue` is an arbitrary JS graph with possible getters, proxies, mutable aliases and callbacks. `Unknown^{s}` cannot hide the capability `s` from the scope checker.

The transition `JsValue -> Unknown` is a runtime snapshot/decoder with limits on size and depth. The first version accepts a supported data graph, copies it, and rejects accessors, functions, unsupported prototypes and cycles; shared subtrees are allowed. Reading Proxy descriptors can run foreign code, so the import is not declared pure and exceptions pass through an explicit mapping. After the import, a pure Schema does not read mutable opaque internals. A single brand check of a `Ref` does not prove the type of its content; a generic opaque leaf needs a certified witness. A separate `DataValue` is not introduced in the first version.

## Public TypeScript interfaces

| Source export | First-version TS API |
| - | - |
| Pure function | An ordinary function with representable TS types |
| Runtime function | A function with an `Effect<A, E, R>` result |
| Representable constraints | An explicit dictionary record or a specialized façade |
| Abstract HKT | A concrete façade; an unrepresentable export is diagnosed |
| Opaque nominal type | A branded/tagged wrapper and safe constructors |
| Schema/codec | The corresponding native Effect Schema value and a limited TS projection |
| Scoped capability/callback | An owner adapter; an unconditional bare TS export is forbidden |

A curried export preserves the stages of the source function. An automatic uncurried façade is allowed when the intermediate applications only create closures or provably finish without an observable failure; the purity of the prefix alone is not enough. For `f x y` the first stage `f x` is executed before `y` is evaluated, while JS first evaluates all arguments of an ordinary call. If the first stage diverges, produces a defect or performs a runtime operation, the transformation may change behavior. In such cases the staged API is kept or an explicit façade with a separately described order is needed. The internal codegen preserves the sequence through the IR.

The default export of a runtime function returns an Effect and starts nothing when the library is imported. The opt-in Promise façade requires provided services, an explicit owner and a cancellation policy. The main form returns `Promise<Result.Result<A, Errors<E>>>`: a selected expected failure becomes a Result, a defect/interruption rejects the Promise with an opaque `RuntimeFailure` with a safe CauseReport. Cancellation is set by a separate signal/handle and preserves quiescence. The Promise gets no fictitious typed rejection. An external façade cannot return a short-lived capability after its owner is closed.

The native `.iface` preserves kinds, constraints, arrows, rows, nominal identities, captures/lifetime and the dictionary ABI. `.d.ts` is a checkable limited projection. A change of data layout, marshalling or of the native Effect ABI version requires an interface compatibility check. An unrepresentable guarantee cannot be silently dropped on export.

## Optimizing the single Effect backend

Flick has one backend, and the standard library is Effect-oriented. Optimization chooses the suitable generation inside it:

| Source code | Generation |
| - | - |
| Pure function | An ordinary JS/TS function without running a fiber and without an Effect wrapper |
| Pure work with Effect data | Calls of Effect's data modules without running the runtime |
| Runtime function | A lazy Effect computation that returns the source result when executed |
| A pure section inside a runtime body | Ordinary expressions inside the deferred body |
| `task` | A first-class Effect with a deferred body |
| External entry point | Providing services and one explicit run of the Effect |

```ts
import { Effect } from "effect"

const normalizeName = (raw: string): string =>
  raw.trim()

const readName = (
  readRawName: () => string
): Effect.Effect<string> =>
  Effect.sync(() => normalizeName(readRawName()))
```

Here `normalizeName` is executed as ordinary JS. The whole call of `readRawName` and the normalization of the result stay inside the lazy runtime body; this example illustrates a synchronous adapter, not a separate way of executing the language.

### Optimization profiles of the single Effect backend

| Profile | What is allowed | Cost |
| - | - | - |
| Conservative lowering | Direct mapping of runtime steps, simple local transformations, ordinary JS for pure bodies | More closures and combinators; convenient as a reference/debug mode |
| Typed IR preserving boundaries — the chosen release mode | Inlining, specialization of dictionaries and callbacks, safe uncurrying, removal of administrative wrappers, local CSE and fusion under proven conditions | Requires analysis of purity, partiality, identity, escape and moment of execution |
| Aggressive specialization after the first version | Additionally local state, statically known services, loops and representations | Costly analysis of aliasing/FFI and a separate justification of each transformation |

All profiles use the same runtime. Pure functions stay ordinary JS; `->{}` does not become a pure arrow. Empty `r/e` do not prove the absence of IO, Ref, suspension, a defect or an observable order.

| Transformation | Condition of admissibility |
| - | - |
| Specialization of a class dictionary | Preserves the chosen instance, equality/hash and the points where arguments are evaluated |
| Uncurrying | Prefixes do not diverge and do not throw before the argument whose evaluation is moved |
| Constant folding | Exact JS Number/BigInt semantics and preservation of the moment of a possible exception |
| CSE / removal of a computation | Purity, admissible partiality and the unobservability of allocation/identity through the FFI are proven |
| Fusion of a pure section | Execution stays inside the same deferred region; the order of observable runtime actions is preserved |
| Merging runtime steps | Handlers, provide, finalizers, explicit yield and the necessary cancellation capabilities are preserved |

Any pass also preserves the lexical owner, the captures, the original Cause on rethrow and quiescence before release. The exact number of allocations and of automatic scheduler ticks is not a contract, but mandatory cancellation points, explicit yields and exactly-once finalizers cannot be removed.

Examples forbidden without separate proof: `(a + b) + c -> a + (b + c)`, `x * 0 -> 0`; memoization of a task's repeated run; merging separate Layer constructions; moving a service lookup across a provide; removing the Schema check of an Unknown on the basis of a TS type name.

The body of a `task` is not executed on construction; two runs remain two runs. The eager API of Effect cannot be substituted mechanically for lazy composition. Moving a partial pure computation can also change the program: for example, an exception at construction instead of an exception during execution.

The observable contract includes the order of source effectful calls, the boundaries of handlers/provide/Scope, the registration of finalizers, explicit yields and the meaning of interruption. The exact automatic fiber schedule and the winner of a genuine race between ready branches are not fixed between builds. The [Effect scheduler](https://effect.website/docs/v4/api/effect/Scheduler#MaxOpsBeforeYield) uses a counter of runtime operations for cooperative yielding, so fusion affects the moments of switching. The compiler must preserve the possibility of cooperative progress and not turn an unbounded sequence of runtime steps into a blocking infinite JS loop.

Number semantics and equality are different foundations: even with an Eq that merges signed zero, an optimization must preserve distinguishable arithmetic results. Stronger modes are possible only with an explicit change of the numeric contract; they are not part of the chosen JS semantics.

### ReScript-style optimizations

The chosen direction is to adapt suitable ReScript optimizations to our own typed IR. Checking of types, effects, instances and regions is done before the transformations; the necessary proofs and source ranges are preserved in the IR.

The [ReScript pipeline](https://github.com/rescript-lang/rescript/blob/81d63a8b78aa9083fed02df6a3d4706313f23cc0/compiler/core/lam_compile_main.ml#L291-L326) that we examined includes alias elimination, flattening, repeated simplification/inlining, removal of let-bindings and scalar replacement. This is a reference point for the passes and their interaction, not a ready-made optimizer for Flick's Effect contracts.

| Priority | Technique | Application in the Flick backend |
| - | - | - |
| First set | Local inlining, beta reduction, alias/constant propagation | Remove helper functions and known applications, preserving the order and moment of execution |
| First set | Arity analysis and a direct worker call | Known full applications bypass intermediate closures; partial application preserves currying |
| First set | Pure self-tail recursion into a loop | Stack-safe pure traversals without depending on the JS engine's TCO |
| First set | Elimination of temporary records/tuples | The object is replaced by fields if its representation/identity is not observed |
| First set | DCE and ES modules suitable for bundlers | Unreachable pure constructions are removed; effects and module initialization are preserved |
| Under an explicit ABI | Unboxed nominal wrappers (`unboxed type`) and distinguishable JS unions (`untagged type`) | Useful for the FFI; canonical Effect types keep their representation |
| Next stage | Limited cross-module inlining | Versioned summaries with arity, effects, captures, ABI and a limit on code growth |
| Next stage | Local elimination of mutable blocks | Only after escape/alias/identity analysis; not a general rule for Effect Ref |

#### Direct calls preserving currying

```flick
add : Number -> Number -> Number
let add left right =
  left + right
```

A possible internal generation:

```js
function add$2(left, right) {
  return left + right;
}

const add = left => right => add$2(left, right);
```

A known full application calls `add$2(x, y)`; a partial one keeps the closure. The worker does not have to become a public TS export.

In [ReScript uncurried calls became the default model](https://rescript-lang.org/blog/uncurried-mode/). We keep the chosen ML semantics of stages: if the first call performs a runtime action, diverges or throws an exception before the next argument, mechanically merging the calls changes the program. So the arity optimization is carried over, not a different semantics of functions.

#### Eliminating temporary allocations

```flick
sum_pair : Number -> Number -> Number
let sum_pair left right =
  let pair = { left = left; right = right }
  pair.left + pair.right
```

A possible internal worker:

```js
function sumPair$2(left, right) {
  return left + right;
}
```

The exact [ReScript SROA pass](https://github.com/rescript-lang/rescript/blob/81d63a8b78aa9083fed02df6a3d4706313f23cc0/compiler/core/lam_pass_sroa.ml) deals with local mutable blocks with verified direct field reads/writes. This is not permission to replace an arbitrary `Effect.Ref` with a variable: a Ref may be shared between fibers, have an external alias or observable identity. To begin with, it is simpler to eliminate provably local immutable records/tuples.

#### Evaluating literal expressions

Like ReScript, the compiler evaluates at compile time expressions composed only of literals: `60 * 60 * 24` becomes `86400`, `"Hello, " ++ "world"` becomes a single string, `not false` becomes `true`, `if true then a else b` becomes the value `a`. The result must match execution in JS bit for bit: IEEE 754 for `+ - * / %` on Number, SameValueZero for `==` on Number, UTF-16 code unit order for `<` on String, truncation toward zero for `/` and `%` on BigInt.

| Expression | Evaluated at compile time |
| - | - |
| Number, BigInt, String, Bool literals; parentheses | The value of the literal |
| Unary `-`, `not`; `+ - * / %` on Number | Yes |
| `**` on Number | Only with a zero exponent or an integer result exactly representable in binary64; otherwise a runtime evaluation remains, because engines' `Math.pow` may differ in the last bit |
| `+ - * / % **` on BigInt | As long as the result fits in 128 bits; division by `0n` and a negative exponent remain a runtime `RangeError` exception (a defect) |
| `==`, `!=`, `<`, `<=`, `>`, `>=` | On Number, BigInt and String; on Bool — only `==` and `!=` |
| `&&`, `\|\|`, `++` | Only when both operands are constant: folding does not discard a computation with an effect |
| `if` with a constant condition | The value of the chosen branch, if it is constant |

Folding is applied to a checked program: a mix of types, for example `1 + 1n`, is rejected by the checker, not by the optimizer.

Literal values. In a string `"…"` the escapes `\n \t \r \\ \" \0 \u{…}` apply. Raw strings `r"…"`, `r#"…"#` have no escapes. In `"""…"""` the following are removed: the line break after the opening quotes, the last line consisting only of spaces before the closing quotes, and the common indentation of non-empty lines (the closing line is counted too); escapes are decoded after that, so `\u{20}` is content, not indentation. A line break inside a literal is always `\n`, including in a source with `\r\n`. A numeric literal is rounded to binary64 the same way as in JS.

#### Optimizing classes

A unique canonical instance allows specializing the dictionary, getting a direct method call and then applying inlining. This part is our adaptation to type classes. With several different dictionaries of the same type the optimizer must preserve the chosen dictionary; the name of the constraint alone is not enough.

#### What preserves the contract

[ReScript compiles tail recursion into loops](https://rescript-lang.org/docs/manual/function/). For Flick runtime code the loop additionally preserves laziness, the boundaries of handlers/Scope and cooperative yielding. Unbounded runtime recursion cannot be unconditionally turned into a synchronous JS loop.

[ReScript DCE](https://rescript-lang.org/docs/manual/introduction/) uses purity information. In Flick an unused `task { Log.info message }` can be removed as a pure description, if the construction itself has no observable action. An executed `Log.info message` cannot be removed because of an unused `Unit` result.

[ReScript unboxed variants](https://rescript-lang.org/docs/manual/variant/) are admissible when the runtime representations are distinguishable. For an opaque newtype in Flick erasure does not lift the Eq/Hash restrictions: special behavior may require a wrapper or a dictionary adapter. Effect's native `Option`, `Result`, `Ref`, `Schema` and `Redacted` do not change their ABI for this optimization.

The representation is declared explicitly and identically in all profiles, because it is an ABI, not an optimizer decision:

```flick
unboxed type UserId = UserId String
  deriving Eq, Hash, Show, Schema

untagged type Key = ByNumber Number | ByName String | ByPath (List String)
```

The value of an `unboxed type` is the field of its single constructor with one field. `opaque unboxed type` hides the constructor from importers in the same way as `opaque type`. An `untagged type` is the field of each constructor; the fields are distinguishable by JS type (`Number`, `String`, `Bool`, `BigInt`, one list/tuple), and the checker rejects an overlap (`FLICK-T020`). Pattern matching checks `typeof`/`Array.isArray` instead of `_tag`. Derived `Eq`, `Ord`, `Hash`, `Show` work on the representation; the wire form of `deriving Schema` for an `unboxed type` is the form of the field, an `untagged type` has no derived witness, and it is assembled with `Schema` combinators. The TS projection is the type of the field or the union of the types of the fields.

In [ReScript int has 32-bit semantics](https://rescript-lang.org/docs/manual/primitive-types/). `Number` in Flick follows JS, so `(a + b) | 0` and replacing an ordinary multiplication with `Math.imul` are forbidden without separate proof.

Any inlining has a budget on size and depth. The cross-module pass uses source interfaces with semantic information, and does not try to reconstruct it from `.d.ts` or the Content Mapper.

After optimization, individual modules may consist only of ordinary JS functions. This is a property of the generated code, not a standalone goal of another backend. When runtime operations appear, Effect, its fibers, scopes, Context and Layers are used.

**FFI/codegen decision:** the chosen ABI, four callback families, an inert Unknown after runtime import, a checkable subset of `.d.ts`, Effect exports and an opt-in Promise façade. Debug lowering serves as the reference; release uses the typed IR and suitable ReScript passes. The runtime adapter pins the exact Effect version, checks compatibility on update and does not replace region contracts with bare TS types.
