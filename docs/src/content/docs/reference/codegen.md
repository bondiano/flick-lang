---
title: "Code generation"
---

`flick build` compiles each module to an ES module (see [TypeScript generation and the FFI boundary](../../spec/08-typescript-and-ffi/), [The Flick compiler and Content Mapper integration](../../spec/11-compiler/)). This page
describes the debug profile, which is the reference lowering: every other
profile must behave the same way.

Debug output is laid out for reading: a bracket group that does not fit in
100 columns, or whose elements span lines, puts each element on a line of
its own, and a lone callback hugs its call. Both profiles leave out the
parentheses precedence does not need. Debug imports a module under its own
name, `Ui.div`, `Ui_Event.exhaust`, unless a constructor of the module or a
JS global has it, and an ignored parameter is `_`; release uses `$m_Ui` and
`$_`. Release output stays compact; read it through its source maps.

## Output

| Path in the output directory | Contents |
| - | - |
| `A/B.js`, `A/B.js.map` | module `A.B` and its source map |
| `_flick/<Name>.js` | the built-in modules the program reaches (`Prelude`, `Platform`, …); with `--emit-dts`, all of them |
| `flick-main.js`, `flick-<name>.js` | the launcher for each app entry: imports its `main` from the defining module and starts it through the runtime, or calls a pure `main` directly; without schema AOT, a pure launcher needs no runtime import |
| `package.json` | `{ "type": "module", "sideEffects": ["./flick-main.js"] }`, listing every runner; `"sideEffects": false` without one |

Module `Main` stays `Main.js`/`Main.ts`; the hyphenated launcher `flick-main.js`/`flick-main.ts` avoids case-insensitive filename collisions, and the old launcher name is no longer emitted.

Without a project, `flick build <files> --out-dir <dir>` writes the same
layout; a root file whose module exports `main` gets the runner. Importing a
module never runs anything: modules only define values. A top-level value
that is a call, a derived instance or an immediately run arrow is marked
`/*#__PURE__*/`, so a bundler drops what the program does not use.

Generated code imports `@flick-lang/runtime` only: `Effect` comes through it.
A module that has no runtime stage imports no `Effect` at all.

## Values

| Source value | JS |
| - | - |
| `Unit`, `()` | `undefined` |
| `Int`, `Float`, `BigInt`, `String`, `Bool` | the JS primitive; a number literal is printed from its value (`0xff` and `1_000` become `255` and `1000`, a negative one is parenthesized), a `BigInt` one keeps its radix and gets `n` |

`Int` arithmetic keeps a value in 32 bits: `a + b` is `(a + b \| 0)`, `a * b`
is `Math.imul(a, b)`, and `/`, `%` and `**` are runtime calls that truncate
and divide by zero to `0`. `/` and `%` on `BigInt` are runtime calls too,
for the zero divisor.
| tuple, `List` | dense array |
| record | plain object with exactly its fields, in the order written |
| ADT value | `{ _tag, _0, _1, … }`, fields in declaration order |
| `unboxed type T = C f` | the field itself |
| `untagged type T = A f \| B g …` | the field of its constructor, told apart by its JS type |
| `type error E = E …` | the same shape: errors are values of their one constructor |
| `Option`, `Result` | native Effect `Option` (`Some`/`None`, `value`) and `Result` (`Success`/`Failure`, `success`/`failure`) |
| `Ref` | a runtime cell, equal only to itself |

A constructor id names the package, module, type and constructor:
`@acme/shapes/App.Shapes.Shape.Circle`; without a package (a file outside a
project, the built-in modules) it is `App.Shapes.Shape.Circle`. The `_tag`
is the constructor's name and the 48 high bits of the FNV-1a 64 hash of its
id, `Circle#7c57f5571734`: the identity stays in the tag at about half its
length. The `.iface` records both. A user type with a `Some` constructor is
never mistaken for `Option`.

`unboxed type` and `untagged type` name the representation of a variant
type for code across the FFI, in every profile. An unboxed type has one
constructor with one field and is that field; an untagged type is the one
field of each constructor, which must differ in JS type — `Float`,
`String`, `Bool`, `BigInt`, or one list or tuple (`FLICK-T020` otherwise).
A pattern tests `typeof` or `Array.isArray` instead of `_tag`; derived
`Eq`, `Ord`, `Hash` and `Show` follow the representation; `deriving Schema`
of an unboxed type has the field's wire form, an untagged one has no
derived witness. The TypeScript projection is the field type, or the union
of the field types.

```flick
unboxed type UserId = UserId String
  deriving Eq, Show, Schema

untagged type Key = ByNumber Float | ByName String | ByPath (List String)
```

`opaque unboxed type Token = Token String` hides the constructor from
importers as `opaque type` does, and keeps the string itself at run time.

`==` is canonical structural equality: SameValueZero on numbers, so
`NaN == NaN` and `0 == -0`; `Float.strict_equal` is `===` and
`Float.same_value` is `Object.is`. `<`, `<=`, `>`, `>=` are the JS
operators; `Ord Float` (`compare`) is the total order with NaN last.
Where the operands are `String`, `Bool` or `BigInt`, `==` is `===`; on a
type whose constructors have no fields it is `a._tag === b._tag`.

## Functions and stages

Each arrow of a curried function is one JS arrow, `(a) => (b) => …`. A pure
stage is plain JS. A runtime stage returns an `Effect.gen` whose runtime
calls are `yield*`; pure calls in it stay plain calls:

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

Evaluation is left to right, stage by stage: a runtime first stage finishes
before the next argument is evaluated. When an operand needs statements
(`match`, a block, a branch with statements), the operands before it become
constants first.

| Construct | JS |
| - | - |
| `task { body }` | `Effect.gen(function* () { … })`, built without running; each `yield*` runs the body again |
| `await e` | `yield* e` |
| a pure function where a runtime one is expected | wrapped: each lifted stage runs in `Effect.sync` |
| `try body with arms` | `$rt.tryWith(body, handler)`: the arms see the first typed failure of the cause |
| `provide layer { body }` | `$rt.provide(layer, body)`: the layer is built before the body |
| `scope [s] { body }` | `$rt.scope((s) => body)`: a lexical owner with a two-phase close, see [runtime.md](../runtime/) |
| a call acquiring into an outer `scope` | `$rt.within(owner, call)`, the owner of that scope's region |
| a supported pure tail call | a loop over parameters, including mutually recursive top-level functions; see below |
| an eligible tail-recursive list walk | an array and an offset, not a copy of the remaining list on every step |
| runtime recursion | through `yield*`, without growing the JS stack |

## Recursion: stack safety is not the whole cost

A call is in tail position when its result is returned without further
work. `sum (acc + x) rest` below is a tail call; `x + sum 0 rest` is not,
because the addition still has to run after the call returns.

```flick
sum : Float -> List Float -> Float
let rec sum acc xs =
  match xs with
  | [] -> acc
  | [x, ...rest] -> sum (acc + x) rest
```

Both debug and release lower the recursive call to a loop. The next
arguments are evaluated from the current iteration's bindings, then
assigned directly; an unchanged argument needs no assignment. This keeps
argument swaps correct without an extra set of per-iteration temporaries.
The right operand of `&&` or `||` is also a tail position when the whole
expression is returned: short-circuiting stays intact.

Because a Flick list is a JS array, stack safety alone would not make
`sum` efficient. Copying `rest` with `slice` on every step copies a
quadratic number of elements. For this traversal the compiler keeps the
original array and advances an offset: matching reads the remaining length
and indexes relative to that offset. The walk is linear, without allocating
a new tail array on each iteration.

This transformation is deliberately narrow: the list parameter must only
be matched in tail position, and `rest` must only be passed to the same
list position of the next loop jump. Passing a different list resets the
offset to zero. A tail returned to a caller or otherwise escaping the loop
still needs its own array; the representation of `List` has not changed.

### Mutual recursion and dictionaries

Pure top-level functions whose tail calls form a cycle share a loop with a
member selector. Each public function enters that loop at its own member:

```flick
rec {
  is_even : Float -> Bool
  let is_even n = if n == 0 then true else is_odd (n - 1)

  is_odd : Float -> Bool
  let is_odd n = if n == 0 then false else is_even (n - 1)
}
```

These parity functions expect a non-negative integer; the loop transform
does not add input validation or make other inputs terminate.

The back-and-forth calls become member changes in a `for (;;) switch`
loop, not a JS frame per step. A constrained function can also loop on a
self call that passes exactly its own class dictionaries. A recursive call
with different dictionaries remains a call; this is not a general promise
to eliminate every recursive call.

A pure loop is synchronous: stack safety does not make a long computation
cooperative or cancellable. Runtime recursion instead runs through Effect
steps, preserving scheduling and interruption.


## Services

`service S` in module `M` is a runtime service key `M.S`. `S.op` looks the
service up at the first runtime stage of the operation and calls the method
of the implementation record; a method of an `S.Impl` value is called
directly. `Layer.succeed S impl` builds the layer.

## Dictionaries

A definition with class constraints takes their dictionaries first:
`render : Show a => a -> String` is `(dShow) => (value) => …`; a use passes
them, `render(<Show Int>)(1)`. A dictionary is an object of the class's
methods with a getter per superclass (`$Eq` of an `Ord` dictionary). An
instance with a context is a function of the context's dictionaries. Derived
instances and the compiler-owned instances of records and tuples are built
by the runtime from the dictionaries of their fields. A higher-rank argument
with a constraint is a function of its dictionaries. The dictionary of
`instance Show Float` is `$i_Show_Float`; a head that is not names
separated by spaces, `Show (List a)`, adds a hash: `$i_Show_List_a_19082212`.

## Foreign functions

`foreign mode name from "module" "export"` imports the export; the curried
Flick function calls it with all arguments of its stages up to the first
runtime one (or `arity n`), dictionaries first. `pure` is a direct call,
`sync` runs in `Effect.sync`, `async` awaits the returned `Promise` — the
innermost owner waits for it to settle even after an interruption (see
[runtime.md](../runtime/)) — and `effect` returns an `Effect`. A relative module is
resolved against the source file. `global` names a global, as in
`foreign sync now from global "Date.now"`.

## Source maps

Each module has a version 3 source map. Definitions, statements, calls,
operators and returns map to the start of their Flick node, so a stack trace
under `node --enable-source-maps` points at `.fl` lines. A map names its
sources relative to itself, `../../src/App/Main.fl`, so no path of the
build machine ships with it.

## TypeScript

`flick build --emit ts` (or `emit = "ts"` in `flick.toml`) writes the same
modules as `.ts` files, importing each other with the `.ts` extension. Each
module declares the TypeScript form of the types it exports and annotates its
exported values with their projected types (see [Public TypeScript interfaces](../../spec/08-typescript-and-ffi/#public-typescript-interfaces)):

| Flick | TypeScript |
| - | - |
| `Int`, `Float`, `String`, `Bool`, `BigInt`, `Unit`, `Never`, `Unknown` | `number`, `number`, `string`, `boolean`, `bigint`, `undefined`, `never`, `unknown` |
| `List a`, `(a, b)`, `{ x : a }` | `ReadonlyArray<A>`, `readonly [A, B]`, `{ readonly x: A }` |
| `Option a`, `Result e a` | Effect `Option.Option<A>`, `Result.Result<A, E>` |
| `a -> b` | `(arg: A) => B`, one arrow per stage |
| `a ->{r; e} b`, `Eff r e a` | `(arg: A) => Effect.Effect<B, E, R>`, `Effect.Effect<A, E, R>`; an empty row is `never` |
| an exported function whose last stage is Sync, `a ->{Atom.Read;} b` | `(arg: A) => Effect.Effect<B, never, Atom.Read>`: a Flick caller still gets the value at once; a TypeScript one gets an Effect that reads the fiber's services, so a direct call no longer fails |
| `Stream r e a` | Effect `Stream.Stream<A, E, R>`, the value itself |
| a variant type, an error | a union of `{ readonly _tag: "<constructor id>"; readonly _0: … }` |
| an opaque, abstract or built-in nominal type, a service | an interface branded with its qualified name |
| `service S` | an interface branded with its name, and in `.d.ts` `$s_S: Context.Service<S, { op: … }>`, the key TypeScript provides an implementation by (`Effect.provideService(program, $s_S, impl)`) |
| a class over types | the interface of its dictionary; a constrained value takes it first |
| `forall a.` | a generic `<A>`; a polymorphic constant uses `never` |
| `forall a b. a -> b -> …` | each type parameter on the first stage that takes it: `<A>(arg: A) => <B>(arg: B) => …`, so a later argument infers its own |

`--emit-dts` (or `emit-dts = true`) writes `A/B.d.ts` beside each module: the
same declarations, `export declare const` for every exported value and
constructor, and the dictionary of each instance without a context. An export
whose type has no TypeScript form is `FLICK-F004`; the declarations never
contain `any`.

`--promise-facade` (or `promise-facade = true`) writes `A/B.promise.js` and
its `.d.ts` for each exported runtime function that needs no service and
whose earlier stages only make closures: `f(a, b, options?)` runs `f a b`
under an owner of its own and returns `Promise<Result<A, E>>`. An expected
failure resolves to `Result.fail`; a defect or an interruption rejects with a
`RuntimeFailure` whose message is the inert report of the cause;
`options.signal` interrupts the run, and the promise settles after cleanup.
An already-aborted signal rejects before the function or its effects run;
completed calls release their abort listeners. Importing either file runs nothing.

A type TypeScript cannot state faithfully — a higher-kinded variable, a
region or a value holding one, an open record — has no projection: the export
stays `any` in `--emit ts`, and `--emit-dts` reports it. Code inside a module
is annotated `any`: the source types live in the Flick checker and its
`.iface`. The output runs as it is on Node (type stripping) and passes
`tsc --noEmit` with `strict`, `allowImportingTsExtensions` and
`module: nodenext`. `@flick-lang/runtime` ships `index.d.ts` for it.

## Release profile

`--profile release` (or `build.profile = "release"`; `flick test` takes the
same flag) runs the optimizer on the typed IR before code generation. It
uses the same runtime and keeps everything a program can observe: the
output, the order of runtime calls, failures, `Exit` and exit codes, and
the module's `.iface`, which comes from the reference lowering. The e2e
corpus runs in both profiles against the same expectations.

| Pass | What it does |
| - | - |
| constant folding | literal expressions with JS semantics bit for bit (`60 * 60 * 24` → `86400`); `show` of a literal `Bool`, `String`, `BigInt` or `Float`, spelled as JS `String` spells it (`"n" ++ show (10.0 + 10.01)` → `"n20.009999999999998"`); a non-finite result is the JS global `Infinity`, `-Infinity` or `NaN`, and `Float.infinity` and `Float.nan` fold as constants; a branch of a constant condition; a result JS might compute differently (`2 ** 0.5`) or would throw (`2n ** -1n`) stays a computation; `Int` arithmetic folds with its wrap-around |
| specialization | a function with numeric class constraints, of the module or of another module of the build (a built-in one such as `List.sum` included), gets a private copy for the numeric instances it is called with, whose methods become the operators of the type; a function of another module is copied only when its body names what its module exports |
| inlining | a known full application of a small pure function of the module, or of a private one used once; the arguments are bound in order first; beta reduction of `(fun x -> …) a` |
| propagation | a local bound to a literal or a name is replaced by it, and so is a top-level value of the module bound to a literal; a pure value that cannot throw moves to its one use when that use surely runs |
| fusion | `Seq.map`, `Seq.filter`, `Seq.flat_map` and `Seq.take` of a lazy `for`, and `List.map`, `List.filter` of a strict one, run in its loop; `Seq.to_list` of a lazy `for` is the strict one, a loop pushing into an array; a `for` over a `for` nests the inner clauses. Their functions are pure, so running them between the clauses is not observable; a known lambda is applied in place, another function is bound once before the loop. A `for` whose body makes Sync calls stays as it is. A pipeline of a `for`, a filter and a map to a list is one loop: 17× faster than the generators of debug (`tools/fusion-bench`) |
| scalar replacement | a local record whose every use reads a field, or a local tuple only destructured, becomes one local per part: no object |
| dead code | unused pure bindings (an unused `task { … }` included) and private top-level values nothing reaches; a runtime call always stays, its result used or not |
| workers | a top-level function whose stages before the last are pure gets `f$n` taking all `n` arguments; a known full application calls it, a partial one keeps the curried `f`; a private function only ever applied fully has no curried form. An exported function of two or more parameters without dictionaries, `foreign` included, exports `f$n` beside `f`, and a full application in another module calls it: the `stages` of its `.iface` summary give `n` |
| dictionaries | a method written in an instance without a context is an exported constant `<instance>$<method>`; a canonical use calls it directly, in the module or an importer; a dictionary passed as a value (an `EqHash` witness) is never replaced |
| foreign | a pure `foreign` that passes its arguments and result as they are is the import itself: `export const div$2 = $f_div`, and a full application in the module calls `$f_div` |
| lone effects | a runtime body that only returns one effect, `Effect.gen(function* () { return yield* x; })`, is `Effect.suspend(() => x)`: built as late, without a generator |
| matches and constructors | the last arm of a `match` without a guard is not tested, since the match is exhaustive; three or more arms that are constructors with only bound fields, the last one possibly any binding, are a `switch (x._tag)` with `default` last; a tagged constructor applied to all its fields is the object it builds, `{ _tag: "Rect#…", _0: 3, _1: 4 }` |

A function whose first stage computes or runs something gets no worker:
joining the stages would move that work after the next argument. The loop
lowerings above are available in both profiles; runtime recursion stays a
chain of `Effect.gen` or `Effect.suspend` steps. Nothing reassociates
arithmetic (`(a + b) + c`), folds `x * 0`, runs or memoizes a task,
merges two `Layer` builds, moves a service lookup across `provide` or
drops a schema check.

## Go target

`emit = "go"` (or `--emit go`) builds an app into one executable without
Node: for each entry, a Go module under `go/<entry>/` of the output, `package
main` with one file per Flick module and the runtime package `flickrt`
beside it, which `flick build` compiles with `go build` into
`flick-<entry>`. Only what `main` reaches is compiled, and a construct the
target does not compile yet is `FLICK-P012` with what it names, never a
runtime stub; for now that is a lazy `for` that reads services, refined
types, `codec` blocks, schemas deriving `Url` or `Form`, and a `foreign`
binding without a Go implementation: in the runtime for the standard
library, which is still much of Effect beyond the core (a `Stream` beyond a
`Seq`, queues, a `Layer` beyond `succeed`, `merge` and `override`, most of
`Codec` and `Http`), or beside the program for its own bindings (see
[FFI](../ffi/#the-go-target)).

Values have the representation of the JS target, in Go terms:

| Flick | Go |
| - | - |
| `Int` | `int32` |
| `Float` | `float64` |
| `BigInt` | `*big.Int`, never mutated |
| `String` | `string` holding WTF-8 |
| `Bool`, `Unit` | `bool`, `flickrt.Unit` |
| a tuple, a `List` | `[]flickrt.Value`; `[x, ...rest]` shares the storage of the list |
| a record | `*flickrt.Record`, its labels in JS key order, copied by `{ r with … }` |
| a constructor | `*flickrt.Data` with the tag of the JS target and its fields |
| a function | `func(flickrt.Value) flickrt.Value` per stage |
| a dictionary | `*flickrt.Dict`; superclasses and default methods made on first use |

Numbers and strings keep the semantics of the JS target: `Int` wraps
around in 32 bits and divides by zero to `0`, `**` truncates a negative
power; `Float` is IEEE double with JS `%` and `Math.pow`; `==` is
SameValueZero; `show` writes a `Float` as JS `String` does
(`0.30000000000000004`, `1e+21`, `-0`), the same digits constant folding
writes. A `String` counts UTF-16 code units in `length`, indices and slices;
a slice may cut a surrogate pair, `++` joins the halves back, strings order
by code units, and writing to stdout replaces a lone half with U+FFFD as
Node does. Normalization and case mapping (`ß` to `SS`, a final sigma,
Turkish dotted i) follow tables `tools/go-unicode/gen.mjs` writes from
Node's ICU; a `RegExp` runs on Go's RE2 engine, with positions in code
units and JS's `$` replacement patterns, and syntax RE2 lacks (lookaround,
backreferences) is a `RegExpError`. `String.locale_compare` orders as
`Intl.Collator("en")` does, by weights the same script reads from Node;
every locale collates as `en`.

A `Seq` restarts on every iteration as on the JS target; a lazy `for` is
one. A `HashMap` or `HashSet` lists its entries in the order Effect's HAMT
walks them, by Effect's `Hash.hash` five bits at a time from the lowest,
so an unsorted `to_list` prints the same on both targets.

A top-level function is a Go function of its dictionaries and all its
parameters; a full application calls it, and a partial one or a use as a
value makes the curried function. The tail calls that are loops on the JS
target (see [Recursion](#recursion-stack-safety-is-not-the-whole-cost))
are loops in Go too: a self call is a `for` that assigns the next arguments
and continues, and a group of functions calling each other is one Go
function looping over a `switch` on the member to run. Go also loops on a
tail call whose last stage runs effects, which the JS target keeps on its
heap step by step. Other recursion uses Go's stack, which grows up to 1 GB.
A defect prints its message and the Flick lines of the stack, from `//line`
directives the generated code carries. Top-level values are assigned by one
function per module, run in the order the modules depend on each other.
The cases of `tests/run` listed in `tests/run/portable.txt` run on both
targets and give the same output.

Effectful code threads a fiber, `*flickrt.Fiber`: a function whose last
stage runs effects takes it first, so a full call stays a plain Go call,
and a `task` is a Go closure of the fiber that runs again on each `await`.
A typed failure, a defect or an interruption is a Go panic carrying a
cause tree like Effect's; `try` recovers it by the rules of the JS target,
and a failing `main` is reported as the JS runner reports it. A service is
its key `M.S`, looked up in the fiber's services when the operation runs,
so a task built outside `provide` and awaited inside it sees the provided
service. Each fiber of `Fiber.spawn`, `parallel`, a race or a `timeout` is
a goroutine. Interruption is cooperative: it is checked at each runtime
call, in blocking waits and at the head of every loop a tail call became,
and `Effect.uninterruptible` defers it. A `scope` closes as on the JS
target: admission ends, children are interrupted and awaited with their
supervisors, the foreign calls they left running end, then finalizers run
last first and their failures join the body's. `Ref`, `Deferred`, `Latch`,
the semaphores and the synchronized refs are safe across goroutines.
SIGINT and SIGTERM interrupt `main` and close its scopes, and the exit code
is the JS runner's: 130 or 143 when only the signal stopped it. What a
program printed is written out before a fiber blocks, and line by line once
fibers run beside `main`.

A schema witness is a value of the runtime's schemas, and `Codec.json`
reads and writes the canonical JSON form of the JS target, with the same
issues, paths and texts, and `Json.parse` fails with V8's messages. An
`Http.Server.serve` listens with Go's `net/http`: each request runs its
handler in a fiber of its own with the services of the call, a failure
answers with the status of the JS target, and closing the scope stops
accepting connections and waits for the running requests.

`flick run` and `flick dev` of a project that emits go build its
executable into the run generation and start it, `dev` rebuilding and
restarting it on each change; `flick test` builds one executable,
`flick-test`, that runs the `tests` of every test module with the node
runner's options, seeds and output. Building needs Go 1.23 or later on
`PATH`, as Gleam needs Erlang or Node: `flick` does not ship a Go
toolchain, and the executable it builds needs nothing installed. It is
built without cgo, so no C compiler is needed either, unless
`CGO_ENABLED` is set.

## Not compiled yet

`sql` and `cli` blocks are checked but not compiled: `flick build` reports
`FLICK-P012`.
