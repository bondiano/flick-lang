---
title: "TypeScript generation and the FFI boundary"
---

The examples of generated code below are the output of the debug profile, the reference lowering; [Code generation](../../reference/codegen/) describes it in full.

## Pure functions

```flick
identity : a -> a
let identity value = value

greet : String -> String -> String
let greet prefix name = prefix ++ ", " ++ name

hello : String -> String
let hello = greet "Hello"
```

```js
export const identity = (value) => value;

export const greet = (prefix) => (name) => prefix + ", " + name;

export const hello = /*#__PURE__*/ greet("Hello");
```

A pure function is an ordinary JS function. Each arrow of a curried function is one JS arrow; an optimization of a full application does not change the order or the moment at which arguments are evaluated.

## ADTs and pattern matching

```flick
with_default : a -> Option a -> a
let with_default fallback value =
  match value with
  | None -> fallback
  | Some x -> x
```

```js
export const with_default = (fallback) => (value) => {
  if (value._tag === "None") {
    return fallback;
  }
  if (value._tag === "Some") {
    const x = value.value;
    return x;
  }
};
```

In this example `Option` is a standard type with the native Effect representation. A user-defined ADT with a similar name or tags remains a different type. The checker ensures exhaustiveness and nominal identity; `readonly` in TS describes the external interface, while immutability across the FFI is ensured by copying at the boundary (see [Value representation](#value-representation-and-callbacks)).

### Variants, guards and or-patterns

A constructor carries zero, one or several fields. The fields are listed after the constructor name in the same way as the arguments when it is applied and in a pattern: `| Circle Point Float`. A field is an atomic type, so a type application and an arrow are parenthesized: `| Branch (List (Tree a))`, `| Handler (a -> b)`. `Circle (Point, Float)` is one tuple field, not two fields. In the ABI the fields occupy `_0`, `_1` and so on in declaration order.

The right-hand side of `type` always lists variants, and the leading `|` is optional: `type Wrapper = Wrap Float` is an ADT with a single constructor. A transparent alias, including a record type, is declared with `type alias`: `type alias UserId = Float`. `opaque type alias` hides the representation outside the module, and `opaque type` with variants hides the constructors. `alias` is a keyword only immediately after `type`.

```flick
type Shape = Circle Point Float | Rect Float Float | Empty

area : Shape -> Float
let area shape =
  match shape with
  | Circle _ radius -> Float.pi * radius ** 2
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

```js
export const load_name = (id) => Effect.gen(function* () {
  const user = yield* $rt.operation($s_Users, "get", 1)(id);
  const name = normalize_name(user.name);
  yield* $rt.operation($s_Log, "info", 1)(name);
  return name;
});
```

A runtime stage is an `Effect.gen` body: each runtime call is `yield*`, and pure calls stay ordinary calls. A service operation such as `Users.get` looks the service up when its runtime stage runs. Generators are how the backend composes Effect, not a requirement on the source syntax.

## HKT at the TypeScript boundary

HKTs and source kinds are checked by the frontend; in the generated JS they are erased or remain in the dictionaries that are passed around. The public TS API exports concrete applications of HKTs and dictionary records. An export whose type TypeScript cannot state faithfully — a higher-kinded variable, a region, an open record — has no projection: `--emit-dts` reports it (`FLICK-F004`), and the declarations never contain `any`. Such an export needs a concrete façade written in Flick.

The notation `F<A>` for an arbitrary type parameter `F` is not a TypeScript feature. A successful check of the TS projection therefore does not replace checking the source program.

## FFI

A `foreign` declaration binds a JS value to a full Flick signature written above it. The mode says how the call runs:

| Mode | Contract |
| - | - |
| `foreign pure` | A direct JS call, trusted to be pure, also at module load |
| `foreign sync` | Deferred until the runtime stage runs; a throw is a defect unless `catch` maps it |
| `foreign async` | The Promise is created when the call runs, not when a `task` is built; the innermost owner waits for it to settle, even after an interruption |
| `foreign effect` | The JS factory is called when the call runs (`Effect.suspend`), and its `Effect` runs in the current fiber |
| `foreign blocked name "reason"` | No binding: using `name` is a compile error (`FLICK-F005`); the other exports of the module stay usable |

Arbitrary input from JS is `JsValue`; a source record or ADT is obtained only through a checked snapshot or codec. Creating a Promise ahead of time would break the coldness of `task`. If external work uses a scoped capability, on cancellation the binding waits for the confirmed stop or completion of the underlying operation: neither the `AbortSignal` itself nor the completion of the interrupted fiber proves it. An uncancellable operation keeps its owner until it finishes.

Direct self tail calls of a pure function, and tail calls between pure top-level functions that form a cycle, become loops that preserve the order of argument evaluation (see [Recursion](../../reference/codegen/#recursion-stack-safety-is-not-the-whole-cost)). Other pure recursion has no general stack-safety guarantee; the module `Loop` (`Loop.loop`, `Loop.Trampoline`) is the explicit tool for it. Runtime recursion runs through suspended Effect steps, without building the whole chain eagerly. A pure loop has no cancellation checkpoints, so an unbounded sequence of runtime steps is never turned into one.

## Importing `.d.ts`

Bindings are written by hand or generated by `flick ffi import <package>` from a package's `.d.ts`, which TypeScript resolves (see [Importing a package's `.d.ts`](../../reference/ffi/#importing-a-packages-dts)). Each selected export gets one of three statuses: supported — a `foreign` binding; needs policy — the `.d.ts` cannot state the contract; unsupported — outside the subset. The last two are generated as `foreign blocked name "reason"`; the supported exports of the package are available even when another is blocked.

The importer infers nothing a declaration cannot show. A call is `sync`, never `pure`; `any` and TS `unknown` become `JsValue`, or are refused by the package's `strict` policy. A Promise result needs a `catch` policy, an empty result a `nullable` one, a callback a `retention` one. An unchecked type is never replaced with `any` or a pure source type, and a TS object shape does not become an immutable source record because of `readonly`. HKTs, source classes and regions are not expressible in a `.d.ts`: a Flick library carries them in its `.iface`.

### An example of an imported function

A hypothetical library declares:

```ts
export declare function lookup(
  id: string
): Promise<string | undefined>;
```

With the policy `undefined = "none"` and `catch = "JsError"`, the importer produces:

```flick
lookup : String ->{; JsError} Option String
foreign async lookup from "package" "lookup"
  arity 1
  catch JsError.from_js
  nullable
```

`undefined -> None` is the metadata line `nullable`, not something inferred from the result type. The importer reads the package's policies from `policy.toml` next to the generated module (`undefined`, `catch`, `cancel`, `mode`, `retention`, `strict`) and never writes it. The Promise is created when the Effect runs, not when a task is built. The signature contains no proof of cancellation; `cancel signal` is added only for a JS API that really supports it.

Another export:

```ts
export declare function nextId(): number;
```

conservatively gets `Unit ->{} Float`. The function may read the clock or change a counter. A pure arrow requires a reviewed contract; it cannot be inferred from the return type alone.

### Reviewed bindings and package updates

The import writes only the generated module. A binding checked by hand lives in a module of its own and carries `reviewed "<pkg>@<ver> sha256:…"`, the fingerprint of the declaration it was checked against. `flick ffi check` imports each package again and reports a generated module that differs from the new import (`FLICK-F006`), a reviewed binding whose declaration changed or disappeared or that lacks what its shape needs (`FLICK-F007`), and a policy for an export the package does not have (`FLICK-F008`).

A callback with unknown retention does not accept a scoped handle or a closure capturing it: `retention call` (only for the duration of the call) or `retention owner` (until the owner of the region closes) states how long JS keeps it. TS `readonly` does not rule out changes through another alias.

## One signature and an explicit mode of the foreign call

The modes are `pure`, `sync`, `async`, `effect` and `blocked` (see [FFI](#ffi)). The full source signature stands above the declaration; `arity`, `this`, `catch`, the nullable mapping, retention and cancellation are set by binding metadata.

```flick
hypot : Float -> Float -> Float
foreign pure hypot from global "Math.hypot"
  arity 2

now_millis : Unit ->{} Float
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

`arity` sets the number of ordinary positional arguments of the JS call. `arity 2` adapts two stages of currying; `arity 0` takes the source `()` and calls JS without arguments. The pair `(Float, Float)` is not unpacked into two arguments by itself. `cancel signal` (`foreign async` only) passes an `AbortSignal` last and aborts it on interruption; the owner still waits for the Promise.

The source is `from global "Math.hypot"` for a global JS value or `from "./fs.js" "readText"` for an export of an ESM module; a relative module is resolved against the source file. Metadata are indented lines under the declaration, one item per line: a name and atom arguments. The known names are `arity n`, `this`, `catch f`, `cancel signal`, `nullable`, `optional i`, `variadic`, `retention call` / `retention owner` and `reviewed "…"`; an unknown name is `FLICK-S011`. The parser checks only the "name and arguments" form; an item that does not fit the signature or the mode is `FLICK-F001`, and a parameter that may hold a scope's capabilities without a `retention` line is `FLICK-F002`. The [FFI reference](../../reference/ffi/#metadata) gives the meaning of each item.

Removing the physical `Unit` argument does not remove the computation that produced it: `now_millis (Log.info message)` first performs the logging, in source order.

`foreign effect` defers the call of the whole JS factory: the TS type of a function with an Effect result does not prove that building it is pure.

```ts
const fetchUser = (id: UserId) =>
  Effect.suspend(() => fetchUserJs(id))
```

The call is executed in the current fiber. A throw from the factory becomes a defect unless the binding translates it into a typed error. A `foreign pure` that builds an `Eff` makes the stronger claim that building it is pure; these declarations differ even with the same TS type.

A throw and a rejected Promise become a typed error only through `catch f`, with `f : JsValue -> E` and `E` in the error row; otherwise they are defects. `E` may hold no capability of a scope (`FLICK-F003`). An import or initialization error of an external module also happens at runtime.

### The Go target

On the Go target (`[build] emit = "go"`) a binding to a module of the program, `./timers.mjs`, calls the Go function of the same name in `timers.go` beside it, which the build copies into the Go module; without that file the binding is `FLICK-P012`. The Go function takes and returns `flickrt.Value` and every argument of the binding at once: `pure`, `sync` and `effect` functions take the executing fiber first, an `async` one takes a `flickrt.Context`, cancelled on interruption, and runs in a goroutine of its own, which the owner waits for as it waits for a Promise. A panic is a defect. `catch`, `this`, `optional`, `variadic` and adapted callbacks are `FLICK-P012` on this target. See [FFI on the go target](../../reference/ffi/#the-go-target).

## Value representation and callbacks

| Source value | ABI |
| - | - |
| Unit | JS `undefined`; a successful foreign result of type `Unit` is `()` whatever JS returns |
| Int, Float, BigInt, Bool, String | The corresponding JS primitives; `Int` is a number kept in 32 bits |
| Tuple | A dense JS array of fixed length |
| List | A dense JS array with an immutable source API |
| Record | A plain JS object with the exact set of own data fields |
| User-defined ADT | An object with `_tag` = `Name#hash`, where hash is the high 48 bits of the 64-bit FNV-1a of the qualified constructor id; the payload is in `_0`, `_1` and so on |
| Nominal wrapper | A tagged object by default; `unboxed type` declares the representation as its field, `untagged type` as fields distinguishable by JS type |
| Native Effect types | `Option`, `Result` and the other Effect types in the native representation of the pinned Effect version |
| Unknown input from JS | An opaque `JsValue` until a checked snapshot |
| Nullable/optional | `nullable` reads a `null` or `undefined` result as `None`; `optional i` passes `None` as `undefined` |
| Methods | `this`, arity, evaluation order and listener identity are preserved |
| Callbacks | Four families: pure, Effect, Promise, scoped event |

The FFI supports fixed uncurried calls, `this`, optional arguments and a variadic tail taken from a `List`. Flick's currying is preserved. The constructor id includes the package, module, type and constructor and is recorded in `.iface` next to the tag; `_tag` carries the constructor name and the hash of this id, so identity is not reduced to a single name. Immutable data is copied at the boundary: every binding outside `@flick-lang/runtime` gets and gives lists, tuples, records and constructed values as copies, its callbacks included. Primitives, `JsValue`, opaque types and functions pass as they are.

The representation of a variant type is declared explicitly, because it is an ABI, not an optimizer decision:

```flick
unboxed type UserId = UserId String
  deriving Eq, Hash, Show, Schema

untagged type Key = ByNumber Float | ByName String | ByPath (List String)
```

The value of an `unboxed type` is the field of its single constructor with one field. `opaque unboxed type` hides the constructor from importers in the same way as `opaque type`. An `untagged type` is the field of each constructor; the fields are distinguishable by JS type (`Float`, `String`, `Bool`, `BigInt`, one list/tuple), and the checker rejects an overlap (`FLICK-T020`). Pattern matching checks `typeof`/`Array.isArray` instead of `_tag`. Derived `Eq`, `Ord`, `Hash`, `Show` work on the representation; the wire form of `deriving Schema` for an `unboxed type` is the form of the field, and an `untagged type` has no derived witness. The TS projection is the type of the field or the union of the types of the fields.

A JS callback is one of **four explicit families**. A runtime arrow with empty rows does not prove synchrony, and a `void` callback never runs an Effect implicitly.

| Family | What JS gets | Who controls execution | Where to use |
| - | - | - | - |
| Pure callback | `(a, b) => c`, adapted to the JS arity of its type | A direct call | Comparator, `map`, predicate, synchronous transformation |
| Effect callback | `(a) => Effect<b, E, R>` | The receiving Effect API | Native Effect combinators; the current fiber and environment are preserved |
| Promise callback | `Js.promise_callback f`: `(a) => Promise<b>` | A child of the scope where it was made | An API that really awaits a Promise |
| Scoped event callback | A listener from `Events.listen`; each call queues an event | The owner of the region and a supervisor | DOM events, WebSocket, event emitter, external notifications |

Only an argument of the binding is adapted: a function inside a record, a list or an `Option` arrives curried. A Promise callback called after its scope began to close is rejected, and the scope waits for the running calls.

### Subscriptions and event handlers

```flick
type Emitter

on : forall s. Emitter -> Events.Listener s Float ->{} Unit
foreign sync on from "./bindings.mjs" "on"
  retention owner

off : forall s. Emitter -> Events.Listener s Float ->{} Unit
foreign sync off from "./bindings.mjs" "off"
  retention call

report : Failure none ->{} Unit
let report failure =
  Platform.stdout_line ("supervisor: " ++ RawCause.describe (Failure.inspect failure))

handle : Float -> Eff none none Unit
let handle n = task { Platform.stdout_line ("handled " ++ show n) }

listen_while : Emitter -> (Unit ->{} Unit) ->{} Unit
let listen_while source work = scope s {
  let listener = Events.listen s (Events.queue 64) report handle
  Events.attach listener (on source) (off source)
  work ()
}
```

`Events.queue 64` is a bounded FIFO, `report` is the supervisor, and `handle` makes a cold task for each event. One worker, a child of the owner, runs the handler task by task; it never runs inside the event source's call. An event that finds the queue full is dropped and reported to the supervisor, as is every failure of the handler. When the owner of the region begins to close, later events are ignored at once, `remove` gets exactly the function `add` got, the worker ends, and only then are the captures released. Re-registration creates a separate listener.

If JS requires an immediate action — for example, a decision about the default action of an event — the asynchronous part cannot promise it in time. A pure callback returns the decision at once, and the scoped processing starts after it.

### Higher-rank callbacks at the FFI

`(forall a. a -> a)` is exported as a TS callback `<A>(value: A) => A`. The polymorphism belongs to the callback itself, not only to the outer function. Class constraints are passed as dictionaries; a `.d.ts` type does not prove the parametricity of an unknown implementation.

For all families the binding preserves the JS arity, `this`, the handler identity for removal, and a consistent conversion of exceptions. Cancellation requires a real protocol, such as `AbortSignal`, and does not arise from the Promise type itself.

Direct FFI and data validation solve different problems. Passing a trusted `HTMLElement` as an opaque handle does not require a codec. HTTP data, configuration or an unknown JS object require validation before being turned into a `User`. A `.d.ts` signature by itself does not check a value and does not prove the absence of mutation.

**`Unknown` and `JsValue`** are separate types. `Unknown` is an unknown source value: the part available for structural traversal is inert and immutable, and opaque leaves remain opaque. `JsValue` is an arbitrary JS graph with possible getters, proxies, mutable aliases and callbacks. `Unknown^{s}` cannot hide the capability `s` from the scope checker.

`Js.snapshot : JsValue ->{; SnapshotError} Unknown` copies supported data — primitives, plain objects, dense arrays — frozen, at most 64 deep and 100 000 values (`Js.snapshot_with` sets both). It reads properties through their descriptors, so a getter never runs; a getter, a function, a cycle, an object of another prototype, a hole in an array, a throwing proxy or a value past the limits is a `SnapshotError` naming the path. A subtree shared by two places is copied once. Reading a Proxy can run foreign code, so the snapshot is a runtime call, not a pure function.

## Public TypeScript interfaces

| Source export | TS API |
| - | - |
| Pure function | An ordinary function with representable TS types, one arrow per stage |
| Runtime function | A function with an `Effect<A, E, R>` result |
| Constrained value | Takes the dictionary of each class first, typed as the interface of the dictionary |
| Opaque or abstract nominal type, service | An interface branded with its qualified name |
| Higher-kinded variable, region, open record | No projection: `FLICK-F004` under `--emit-dts` |

A curried export preserves the stages of the source function. For `f x y` the first stage `f x` is executed before `y` is evaluated, while JS first evaluates all arguments of an ordinary call. So an uncurried worker `f$n` exists only for a function whose stages before the last only make closures; if the first stage could diverge, fail or perform a runtime operation, joining the stages could change behavior.

The default export of a runtime function returns an Effect and starts nothing when the library is imported. The opt-in Promise façade (`--promise-facade`) exists for an exported runtime function that needs no service and whose earlier stages only make closures: `f(a, b, options?)` runs the function under an owner of its own and returns `Promise<Result<A, E>>`. An expected failure resolves to a failed `Result`; a defect or an interruption rejects the Promise with a `RuntimeFailure` carrying an inert report of the cause. `options.signal` interrupts the run, and the Promise settles after cleanup. The Promise gets no fictitious typed rejection. See [TypeScript](../../reference/codegen/#typescript).

The native `.iface` preserves kinds, constraints, arrows, rows, nominal identities, captures/lifetime and the dictionary ABI. `.d.ts` is a checkable limited projection. A guarantee TypeScript cannot represent is reported, not silently dropped.

## Release optimizations

The release profile (`--profile release`) runs the optimizer on the typed IR before code generation, after types, effects, instances and regions are checked. It uses the same runtime as the debug profile and keeps everything a program can observe: the output, the order of runtime calls, the boundaries of handlers, `provide` and `scope`, the registration of finalizers, failures, `Exit` and exit codes, and the `.iface` of each module. The passes are listed in [Release profile](../../reference/codegen/#release-profile).

Only pure code is rewritten. No pass reassociates arithmetic (`(a + b) + c`), folds `x * 0`, runs or memoizes a task, merges two `Layer` builds, moves a service lookup across `provide`, drops a schema check or removes a runtime call whose result is unused. The body of a `task` is not executed on construction; two runs remain two runs. The exact number of allocations and the automatic fiber schedule — including the winner of a race between branches ready at once — are not part of the contract; mandatory cancellation points, explicit yields and exactly-once finalizers are.

Float semantics and equality are different foundations: even with an `Eq` that merges signed zero, an optimization preserves distinguishable arithmetic results.

### Evaluating literal expressions

Expressions built only from literals are evaluated at compile time — by the checker where it needs a constant, and by constant folding in release: `60 * 60 * 24` becomes `86400`, `"Hello, " ++ "world"` becomes a single string, `not false` becomes `true`, and `if` with a constant condition becomes the chosen branch. The result matches execution in JS bit for bit: IEEE 754 for `+ - * / %` on Float, SameValueZero for `==` on Float, UTF-16 code unit order for `<` on String, truncation toward zero for `/` and `%` on BigInt.

| Expression | Evaluated at compile time |
| - | - |
| Float, BigInt, String, Bool literals; parentheses | The value of the literal |
| Unary `-`, `not`; `+ - * / %` on Float | Yes |
| `**` on Float | Only with a zero exponent or an integer result exactly representable in binary64; otherwise it stays a runtime computation, because engines' `Math.pow` may differ in the last bit |
| `+ - * / % **` on BigInt | While the result fits in 128 bits; `/` and `%` by `0n` are `0n`, as at run time; a negative exponent stays a runtime `RangeError` (a defect) |
| Arithmetic on `Int` | Yes, wrapping around in 32 bits |
| `==`, `!=`, `<`, `<=`, `>`, `>=` | On Float, BigInt and String; on Bool — only `==` and `!=` |
| `&&`, `\|\|`, `++` | Only when both operands are constant |

Folding is applied to a checked program: a mix of types, for example `1 + 1n`, is rejected by the checker, not by the optimizer.
