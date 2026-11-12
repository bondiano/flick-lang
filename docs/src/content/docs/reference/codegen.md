---
title: "Code generation"
---

`flick build` compiles each module to an ES module (see [TypeScript generation and the FFI boundary](../../spec/06-typescript-and-ffi/), [Compiler architecture](../../contributing/implementation-decisions/#compiler-architecture)). This page
describes the debug profile, which is the reference lowering: every other
profile must behave the same way.

Debug output is laid out for reading: a bracket group that does not fit in
100 columns, or whose elements span lines, puts each element on a line of
its own, and a lone callback hugs its call. Both profiles leave out the
parentheses precedence does not need. Debug imports a module under its own
name, `Ui.div`, `Ui_Event.exhaust`, unless a constructor of the module or a
JS global has it, and an ignored parameter is `_`; release uses `$m_Ui` and
`$_`. Release output stays compact; read it through its source maps.

A factory that captures preceding local values may require extra physical
files such as `App/Factory$lexical$123.js`. These contain the original prefix
bindings; the normal module re-exports them. Results import the prefix directly
so an enclosing module may itself use a result without an initialization
cycle. Constructor tags, service keys and type identities still use the Flick
module path. Go orders individual initializers by their value dependencies.

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

Code built by default or with `--rt` imports `@flick-lang/runtime/rt`.
An explicit `effect = "effect"` build imports `@flick-lang/effect-ts`; the adapter provides `Effect`.
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
| `untagged type T = A = "a" \| B = "b"` | the literal of its constructor, a string or a number |
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

An enum is an `untagged type` whose constructors are literals: each value
is its literal at run time, all strings or all `Int`s, distinct
(`FLICK-T020`; an `Int` out of range is `FLICK-T003`). A constructor has no
fields, the type no parameters, and `Get "GET"` without its `=` gets the
hint to write it. A `match` compares the value, as `==` does; derived `Ord`
follows the order of declaration and `Show` gives the constructor's name.
The compiler gives each enum its instance of `Enum.Enum t b`, a program
declares none (`FLICK-C012`), and its methods are the companions
`T.value`, `T.parse` (an `Err` for an unknown literal; of a constant it is
checked at compile time, `FLICK-D006`) and `T.all`, in the order of
declaration. `opaque untagged type` keeps the constructors to the module
and the companions open.

```flick
untagged type Method = Get = "GET" | Post = "POST" | Delete = "DELETE"
  deriving Eq, Ord, Show

untagged type Priority = Low = 1 | High = 2

literals : Enum.Enum t String => List t -> List String
// `literals Method.all` is `["GET", "POST", "DELETE"]`
let literals values = List.map Enum.value values
```

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
# type UserId = UserId String
# type alias User = { name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# service Log =
#   info : String ->{} Unit
# normalize_name : String -> String
# let normalize_name name = String.trim name
#
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
`foreign now from global "Date.now"`.

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
exported values with their projected types (see [Public TypeScript interfaces](../../spec/06-typescript-and-ffi/#public-typescript-interfaces)):

| Flick | TypeScript |
| - | - |
| `Int`, `Float`, `String`, `Bool`, `BigInt`, `Unit`, `Never`, `Unknown` | `number`, `number`, `string`, `boolean`, `bigint`, `undefined`, `never`, `unknown` |
| `List a`, `(a, b)`, `{ x : a }` | `ReadonlyArray<A>`, `readonly [A, B]`, `{ readonly x: A }` |
| `Option a`, `Result e a` | Effect `Option.Option<A>`, `Result.Result<A, E>` |
| `a -> b` | `(arg: A) => B`, one arrow per stage |
| `a ->{r; e} b`, `Eff r e a` | `(arg: A) => Effect.Effect<B, E, R>`, `Effect.Effect<A, E, R>`; an empty row is `never` |
| an exported function whose last stage is Sync, `a ->{Atom.Read} b` | `(arg: A) => Effect.Effect<B, never, Atom.Read>`: a Flick caller still gets the value at once; a TypeScript one gets an Effect that reads the fiber's services, so a direct call no longer fails |
| `Stream r e a` | Effect `Stream.Stream<A, E, R>`, the value itself |
| a variant type, an error | a union of `{ readonly _tag: "<constructor id>"; readonly _0: … }` |
| an enum; an opaque one | the union of its literals, `"GET" \| "POST"`; `string` or `number` branded with its qualified name |
| an opaque, abstract or built-in nominal type, a service | an interface branded with its qualified name |
| `service S` | an interface branded with its name, and in `.d.ts` `$s_S: Context.Service<S, { op: … }>`, the key TypeScript provides an implementation by (`Effect.provideService(program, $s_S, impl)`) |
| a class over types | the interface of its dictionary; a constrained value takes it first |
| `forall a.` | a generic `<A>`; a polymorphic constant uses `never` |
| `forall a b. a -> b -> …` | each type parameter on the first stage that takes it: `<A>(arg: A) => <B>(arg: B) => …`, so a later argument infers its own |

### The form of a type

`@ts` before the declaration of a type or a service says what the type is in
TypeScript. Such a type is not declared: every module that names it writes
the form, and imports only what the forms it wrote name.

```flick
@ts "$Dayjs"
@ts import "dayjs" "Dayjs"
type Moment

@ts "Promise<readonly [{0}, $Handle]>"
@ts import "./fs.d.mts" "Handle"
type Later a

@ts none
type Cursor
```

| Mark | Meaning |
| - | - |
| `@ts "form"` | the form: `{i}` is the type's argument `i` (from 0), `{i:row}` that argument as a row of services or errors; `rt "form"` after it is the form on Flick's own runtime, when it differs |
| `@ts import "module" "Export"` | a `$Export` of the form is that export of the module, named as `foreign … from` names one: a package as written, a file by its path from the source of the module that declares the type; `rt "Other"` after it is the export on Flick's own runtime |
| `@ts none` | the type has no form: an export whose type names it is `FLICK-F004` under `--emit-dts` |

A form is the text of a type and nothing else: names, `.`, `|`, `&`, `?`,
`:`, `=>` and the brackets `<…>`, `(…)`, `[…]`, with a `,` inside brackets
only. A string literal and an object type are not forms; name such a type
in a declaration file and import it. A form that does not read, a place past
the parameters of the type, a `$Export` without its import, and two forms
for one type are `FLICK-S014`. The compiler does not check that the form is
true of the values: that is the word of the package, as the signature of a
`foreign` is.

The `.d.ts`, `.d.mts` and `.d.cts` files of a library ship beside its
modules, as its `.mjs` and `.cjs` do. The built-in modules give their types
their forms the same way, as `@ts "ReadonlyArray<{0}>"` before `type List a`;
only they name the runtime by a word, `@ts import runtime "Option"`
(`FLICK-R016` elsewhere).

`--emit-dts` (or `emit-dts = true`) writes `A/B.d.ts` beside each module: the
same declarations, `export declare const` for every exported value and
constructor, and the dictionary of each instance without a context. An export
whose type has no TypeScript form is `FLICK-F004`; the declarations never
contain `any`.

In `.d.ts`, the `///` comment of an exported type, constructor or value is its
JSDoc, its Markdown as written, also for a library taken from the package
cache. A `type alias` of a record is an `export interface`, which TypeScript
names on hover; unlike an object type it has no implicit index signature, so
it is not assignable to `Record<string, unknown>`. Other aliases stay
`export type`.

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
`module: nodenext`. `@flick-lang/effect-ts` ships `index.d.ts` for it.

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
| devirtualization | a default an instance takes becomes its own method when, with the instance and the dictionaries of its default signature known, its code is a function or data, so `deriving Default` through `Generic` compiles to the instance written by hand; a method of a known instance is its code where that is small data, or a function taking apart a known constructor; a `match` of a known constructor is the arm it takes |
| specialization | a function with numeric class constraints, of the module or of another module of the build (a built-in one such as `List.sum` included), gets a private copy for the numeric instances it is called with, whose methods become the operators of the type; a function of another module is copied only when its body names what its module exports |
| inlining | a known full application of a small pure function of the module or of a built-in one (core, std) whose body names only what that module exports (`Int.min 3 5` is `3`), or of a private one used once; the arguments are bound in order first; beta reduction of `(fun x -> …) a`, and of a runtime call of a function whose one stage runs its body; `x |> f` is the call `f x` when evaluating `x` or `f` shows nothing, so `f` inlines and gets its worker; a composition `f >> g` applied is `g (f x)` |
| known shapes | a function of a built-in module that chooses what it gives with a `match`, bigger than its call, is inlined only where that spares an object: given a function written there that it calls once, which is then called where it is written; given a constructor, or a choice between constructors, that it takes apart; or when its result is taken apart at once. `a \|> f x` is inlined as `f x a` is, with `a` evaluated first. A `match` of a local bound to a constructor reads the constructor, and a `match` of a choice (an `if`, a `match`, a block) every branch of which ends in a constructor moves into the branches, an arm more than one branch takes being copied only when small; the parts of a constructor are named first, so they are evaluated where they were. `value \|> Option.filter p \|> Option.map f \|> Option.with_default 0` with `p` and `f` written there is two tests: no function, no `Some`. An arm that only names what is taken apart, itself a name or a literal (`\| n when n % 3 == 0 -> …` of `match n`), reads it by the name it has |
| propagation | a local bound to a literal or a name is replaced by it, and so is a top-level value of the module bound to a literal; a pure value that cannot throw moves to its one use when that use surely runs; a function applied to names but not yet to all its arguments, which only builds a closure, moves into the function that completes the call, so `List.map f >> List.filter p` calls both with all their arguments |
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
drops a schema check. The automatic fiber schedule, and so the winner of a
race between branches ready at once, may differ between the profiles; the
cancellation points, explicit yields and finalizers of the program do not.

## Go target

`emit = "go"` (or `--emit go`) builds an app into one executable without
Node: for each entry, a Go module under `go/<entry>/` of the output, `package
main` with one file per Flick module and the runtime package `flickrt`
beside it, which `flick build` compiles with `go build` into
`flick-<entry>`. `go build` compiles every file of a package, so the module
lists what the build wrote in `go/<entry>/.flick-files`, and the next build
into the same output removes the listed files it no longer writes; files it
did not write stay. Only what `main` reaches is compiled, and a construct the
target does not compile yet is `FLICK-P012` with what it names, never a
runtime stub; for now that is a lazy `for` that reads services,
schemas deriving `Form`, and a `foreign` binding without a
Go implementation: in the runtime for the standard library, part of which
has none yet beyond the core (most of `Stream`, queues, schedules beyond
`recurs`, parts of `Codec`), or beside the program for its own bindings
(see [FFI](../ffi/#the-go-target)). Modules of the js target only are
`FLICK-P030` already in `flick check`.

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
| a constructor | `*flickrt.Data` with the tag of the JS target; `Fields()` is its fields, up to two of them held in the value itself |
| a function | `func(*flickrt.Fiber, flickrt.Value) flickrt.Value` per stage; one of two or three arguments whose stages before the last are pure is `flickrt.Fn2` or `flickrt.Fn3`, which takes them at once |
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
the Turkish and Azeri i, Lithuanian dots, Greek capitals without accents)
follow tables `tools/go-unicode/gen.mjs` writes from Node's ICU, and a
locale tag is checked and canonicalized as V8 does, with its `RangeError`
for an invalid one; a `RegExp` runs on Go's RE2 engine, with positions in code
units and JS's `$` replacement patterns, and syntax RE2 lacks (lookaround,
backreferences) is a `RegExpError`. `String.locale_compare` orders as
`Intl.Collator` does: the Unicode Collation Algorithm on CLDR 48's root
collation and its tailorings for the languages of 52 common locales, which
`tools/go-collate/gen.mjs` builds as ICU does; a tag of another language
collates as the default locale, and `-u-co-…` extensions are ignored.
`DateTime` formats from tables
`tools/go-intl/gen.mjs` writes from Node: the languages of 52 common
locales with every regional variant ICU has, a tag resolved as V8 resolves
it, zone names by metazone (`CET`, `Central European Standard Time`) at the
offsets a zone has in 2024, and an offset (`+01:00`, `+0100`) taken as a
zone's name. A date in text (`DateTime.make`,
`Config.date`, HTTP dates) is read by a port of V8's `Date.parse`, legacy
forms included; a date-time without an offset is local time, Go's
`time.Local`. `Cli.file_parse` reads INI, TOML and YAML with ports of
the parsers of the JS target, their errors included.

A pattern RE2 reads still means something else in a few places, because
RE2 has no JS mode and Flick passes the source on as written:

- `\s` is ASCII whitespace without `\v`; `.` matches `\r`, U+2028 and
  U+2029; `^` and `$` with `m` break lines only at `\n`.
- There is no non-`u` mode: `.` and a class take a whole code point, never
  one surrogate half, and `\p{…}` is a Unicode class read from Go's
  Unicode 15 tables rather than the literal `p{…}`.
- `i` folds case as Unicode simple folding, so `ſ` matches `s` and the
  Kelvin sign matches `k`; JS keeps those apart.
- `replace_all` skips an empty match right after a non-empty one: `a*` on
  `"baaac"` gives `-b-c-`, JS gives `-b--c-`.
- A replacement reads `$$`, `$&`, `` $` ``, `$'` and `$n`, not `$<name>`.
- Flags other than `g`, `i`, `m`, `s` and `y` are ignored rather than a
  `SyntaxError`, so `d`, `u` and `v` change nothing.
- A `RegExpError` carries RE2's text, not V8's. It is also what lookaround,
  backreferences, `\u…`, `\c…`, `[\b]`, `[^]` and a repeat above 1000
  give, all of which JS accepts.

A `Seq` restarts on every iteration as on the JS target; a lazy `for` is
one. A `HashMap` or `HashSet` lists its entries in the order a HAMT
walks them, by `Hash.hash` five bits at a time from the lowest,
so an unsorted `to_list` prints the same on both targets.

A top-level function is a Go function of its execution fiber, dictionaries
and all its parameters; a full application calls it, and a partial one or a
use as a value makes a function of values around it. A parameter that is an
`Int`, a `Float`, a `Bool` or a `String`, and a result that is one, has its
Go type there (`func F(fb *flickrt.Fiber, n int32) int32`), so a number
passed to a known function or returned by it is no `flickrt.Value` made for
the call. The tail calls that are loops on the JS
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

Every generated Flick function takes its execution fiber, `*flickrt.Fiber`,
first, including pure functions and callbacks. A saved function uses the fiber
of its invocation, not its creation, so URL rendering observes the current
redaction names without process-global state. Pure stages remain pure.
A `task` is a Go closure of the fiber that runs again on each `await`.
A typed failure, a defect or an interruption is a Go panic carrying a
cause tree; `try` recovers it by the rules of the JS target,
and a failing `main` is reported as the JS runner reports it. A service is
its key `M.S`, looked up in the fiber's services when the operation runs,
so a task built outside `provide` and awaited inside it sees the provided
service. Each fiber of `Fiber.spawn`, `parallel`, a race or a `timeout` is
a goroutine. Interruption is cooperative: it is checked at runtime effect
calls, in blocking waits and at the head of effectful tail-call loops;
pure loops remain checkpoint-free. `Effect.uninterruptible` defers it. A `scope` closes as on the JS
target: admission ends, children are interrupted and awaited with their
supervisors, the foreign calls they left running end, then finalizers run
last first and their failures join the body's once. Layer finalizers report
only cleanup failures and still observe the original exit. `Ref`, `Deferred`, `Latch`,
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
accepting connections and waits for the running requests. Each request owns
its resources through response-body streaming. After the grace period
(20 seconds by default), remaining requests are interrupted and joined,
including their finalizers, before the server owner's resources are released.
The grace timeout does not bound cleanup or uninterruptible work. The rest of
`Http` and `HttpApi` follows the JS target: headers, cookies,
URLs, the router's defaults, middleware, multipart forms, static files,
streamed bodies, the client on `net/http` (interrupting a fiber cancels its
request), typed `HttpApi` clients and the same OpenAPI document. What
`net/http` decides differs: a response's status text is always the
standard one, and the client sends a request's headers sorted by name. The
server sees a request's headers in the order they came, as Node does, over
HTTP/1.x; where it cannot follow a connection's framing they come with the
host first and the others by name. Compression with
brotli and zstd uses the Go modules `andybalholm/brotli` and
`klauspost/compress`, which a program requires only when it reaches
`Http.Middleware.compression`; its first build downloads them unless the
module cache has them.

URL rendering and HTTP error details use the current fiber's
`Http.UrlParams.with_redacted_names`, including nested and concurrent scopes;
an error retains its masked URL after leaving the scope. Router constraints
and composite parameters are checked before selecting a handler. Constraints
use RE2: unsupported JavaScript regex syntax, including mixed regex/composite
segments that need lookaround for a multi-character separator, is rejected
when the route is constructed, never treated as an unrestricted parameter.

`FileSystem.node` implements `access`, `copy`, `copy_file`, `chmod`, `chown`,
`glob`, `link`, `open`, `read_link`, `real_path`, `symlink`, `truncate` and
`utimes`; `open` belongs to its surrounding scope. `noFollow` is atomic on
POSIX and a typed `BadArgument` on Windows, as in Node. Windows `chown`
reports Go's unsupported-operation error rather than Node's no-op success.
File-handle foreign operations not yet ported remain `FLICK-P012`;
`stat` metadata remains partial.
Glob's POSIX classes are checked on the minimum Go 1.23, including unassigned
code points. `Stream.watch` uses inotify on Linux, kqueue on macOS and
`ReadDirectoryChangesW` on Windows. Unlike Node's FSEvents, kqueue does
not coalesce events, and a file created as its new directory is listed
may be reported twice. Where the system has no notifications, or runs out
of watches or descriptors, the path is polled every 50 ms; Node fails
there.
`Path.node` and the paths of `Cli` flags are Node's win32 paths on Windows
(`tools/go-path/gen.mjs` checks them against `path.win32`), POSIX elsewhere.

`flick run` and `flick dev` of a project that emits go build its
executable into the run generation and start it, `dev` rebuilding and
restarting it on each change; `flick test` builds one executable,
`flick-test`, that runs the `tests` of every test module with the node
runner's options, seeds and output. Building needs Go 1.23 or later on
`PATH`, as Gleam needs Erlang or Node: `flick` does not ship a Go
toolchain, and the executable it builds needs nothing installed. It is
built without cgo, so no C compiler is needed either, unless
`CGO_ENABLED` is set.
