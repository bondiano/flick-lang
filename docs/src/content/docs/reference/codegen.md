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
| `_flick/<Name>.js` | the built-in modules the program reaches (`Prelude`, `Platform`, …); with `--declarations`, all of them |
| `flick-main.js`, `flick-<name>.js` | the launcher for each app entry: imports its `main` from the defining module and starts it through the runtime, or calls a pure `main` directly; without schema AOT, a pure launcher needs no runtime import |
| `package.json` | `{ "type": "module", "sideEffects": ["./flick-main.js"] }`, listing every runner; `"sideEffects": false` without one |

Module `Main` stays `Main.js`/`Main.ts`; the hyphenated launcher `flick-main.js`/`flick-main.ts` avoids case-insensitive filename collisions, and the old launcher name is no longer emitted.

Without a project, `flick build <files> --out-dir <dir>` writes the same
layout; a root file whose module exports `main` gets the runner. Importing a
module never runs anything: modules only define values. A top-level value
that is a call, a derived instance or an immediately run arrow is marked
`/*#__PURE__*/`, so a bundler drops what the program does not use.

Code built by default or with `--runtime rt` imports `@flick-lang/runtime/rt`.
An explicit `[target.js] runtime = "effect"` (or `--runtime effect`) build imports `@flick-lang/effect-ts`; the adapter provides `Effect`.
A module that has no runtime stage imports no `Effect` at all.

## Values

| Source value | JS |
| - | - |
| `Unit`, `()` | `undefined` |
| `Int`, `Int8`, `Int16`, `Int32`, `UInt8`/`Byte`, `UInt16`, `UInt32`, `Float`, `Float32` | JS `number`; a number literal is printed from its value (`0xff` and `1_000` become `255` and `1000`, a negative one is parenthesized); sub-32-bit and `UInt32` operations mask or sign-extend explicitly (`<< 24 >> 24`, `<< 16 >> 16`, `& 0xff`, `& 0xffff`, `>>> 0`, `Math.imul(...) >>> 0`) and `Float32` rounds via `Math.fround(...)` |
| `Int64`, `UInt64`, `BigInt` | JS `bigint`; `Int64` and `UInt64` wrap via `BigInt.asIntN(64, …)` / `BigInt.asUintN(64, …)` and format without an `n` suffix via `showInt64` / `showUInt64`; `BigInt` keeps arbitrary precision and appends `n` in `show` |
| `String`, `Bool` | the JS primitive |

Fixed-width integer arithmetic keeps every value within its width: `Int` (`Int32`) uses `(a + b \| 0)` and `Math.imul(a, b)`, `Int8`/`Int16`/`UInt8`/`UInt16`/`UInt32` mask or sign-extend in JS `number`, `Int64`/`UInt64` wrap with `BigInt.asIntN(64, …)` / `BigInt.asUintN(64, …)`, and `/`, `%` and `**` call runtime helpers that truncate and divide by zero to `0` (with signed `min / -1 = min` and `min % -1 = 0`). `/` and `%` on `BigInt` are runtime calls too, for the zero divisor.
| tuple, `List` | dense array |
| record | plain object with exactly its fields in canonical sorted UTF-16 label order (field expressions evaluate in source order; array-index keys, if any, come first numerically per ECMAScript) |
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
| `await e` | a runtime call to the selected `Awaitable.await` implementation |
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
instance with a context is a function of the context's dictionaries. A
derived `Eq`, `Ord`, `Hash` or `Show` has its methods written out for its
type, a `match` on the constructors that asks the instance of each field,
and is compiled as any instance is; the compiler-owned instances of records
and tuples are built by the runtime from the dictionaries of their fields.
A higher-rank argument
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

### Value ABI (`flick/js`)

Direct `foreign` bindings and `@native` overrides are checked against the
versioned value-ABI table published by `flick/js` (`FLICK-F013` for unsupported
types or directions, `FLICK-F014` for ABI version mismatches). Rendered from
the `flick/js` backend descriptor:

<!-- value-abi:flick/js:en:start -->
**ABI:** `flick.javascript-ffi@0.1.0` · **Opaque fallback:** `unknown` · **Host `catch` type:** `unknown` (lang item `js_value`)

| Key | Flick | Host | Conversion | Direction | Notes |
| - | - | - | - | - | - |
| `unit` | `Unit` | `undefined` | `none` | `both` | JS `undefined`; a successful foreign result of type `Unit` is `()` whatever JS returns |
| `bool` | `Bool` | `boolean` | `none` | `both` | JS `boolean` primitive |
| `int` | `Int` | `number` | `none` | `both` | JS `number` kept in 32-bit signed integer range (`Int32`) |
| `float` | `Float` | `number` | `none` | `both` | JS IEEE 754 binary64 `number` |
| `bigint` | `BigInt` | `bigint` | `none` | `both` | JS arbitrary-precision `bigint` |
| `int8` | `Int8` | `number` | `none` | `both` | JS `number` masked/sign-extended to signed 8-bit integer |
| `int16` | `Int16` | `number` | `none` | `both` | JS `number` masked/sign-extended to signed 16-bit integer |
| `int64` | `Int64` | `bigint` | `none` | `both` | JS `bigint` wrapped to signed 64-bit integer |
| `uint8` | `UInt8 (Byte)` | `number` | `none` | `both` | JS `number` masked to unsigned 8-bit integer |
| `uint16` | `UInt16` | `number` | `none` | `both` | JS `number` masked to unsigned 16-bit integer |
| `uint32` | `UInt32` | `number` | `none` | `both` | JS `number` masked to unsigned 32-bit integer |
| `uint64` | `UInt64` | `bigint` | `none` | `both` | JS `bigint` wrapped to unsigned 64-bit integer |
| `float32` | `Float32` | `number` | `none` | `both` | JS `number` rounded to IEEE 754 binary32 |
| `string` | `String` | `string` | `none` | `both` | JS UTF-16 `string` primitive |
| `never` | `Never` | `never` | `none` | `both` | Uninhabited bottom type |
| `js_value` | `JsValue` | `unknown` | `none` | `both` | Opaque `JsValue` passed as-is until a checked snapshot |
| `unknown` | `Unknown` | `unknown` | `copy` | `both` | Inert immutable Flick value; deep-copied across the boundary |
| `tuple` | `(a, b, …)` | `readonly [A, B, …]` | `copy` | `both` | Dense JS array of fixed length; deep-copied across the boundary |
| `list` | `List a` | `ReadonlyArray<A>` | `copy` | `both` | Dense JS array with an immutable source API; deep-copied across the boundary |
| `option` | `Option a` | `Option.Option<A>` | `copy` | `both` | Native Effect `Option`; `nullable` reads `null`/`undefined` as `None` and `optional i` passes `None` as `undefined` |
| `result` | `Result e a` | `Result.Result<A, E>` | `copy` | `both` | Native Effect `Result` representation of the pinned Effect version |
| `eff` | `Eff r e a` | `Effect.Effect<A, E, R>` | `none` | `both` | Native Effect `Effect` representation of the pinned Effect version |
| `layer` | `Layer ri e ro` | `Layer.Layer<RO, E, RI>` | `none` | `both` | Native Effect `Layer` representation of the pinned Effect version |
| `exit` | `Exit e a` | `Exit.Exit<A, E>` | `none` | `both` | Native Effect `Exit` representation of the pinned Effect version |
| `failure` | `Failure e` | `Cause.Cause<E>` | `none` | `both` | Native Effect `Cause` representation of the pinned Effect version |
| `function` | `a -> b, a ->{r; e} b` | `(a, …) => b` | `none` | `both` | Callbacks adapted to JS arity across four families: pure, Effect, Promise, scoped event |

Records and user-defined ADTs have no stable layout and are not in this table. On this backend they may still cross bindings as deep-copied plain JS objects / tagged objects (`_tag` = `Name#hash`, fields `_0`, `_1`, …); `unboxed type` and `untagged type` declare explicit representations.
<!-- value-abi:flick/js:en:end -->

## Source maps

Each module has a version 3 source map. Definitions, statements, calls,
operators and returns map to the start of their Flick node, so a stack trace
under `node --enable-source-maps` points at `.fl` lines. A map names its
sources relative to itself, `../../src/App/Main.fl`, so no path of the
build machine ships with it.

## TypeScript

`flick build --typescript` (or `[target.js] typescript = true` in `flick.toml`) writes the same
modules as `.ts` files, importing each other with the `.ts` extension. Each
module declares the TypeScript form of the types it exports and annotates its
exported values with their projected types (see [Public TypeScript interfaces](../../spec/06-typescript-and-ffi/#public-typescript-interfaces)):

| Flick | TypeScript |
| - | - |
| `Int`, `Int8`, `Int16`, `Int32`, `UInt8`/`Byte`, `UInt16`, `UInt32`, `Float`, `Float32` | `number` |
| `Int64`, `UInt64`, `BigInt` | `bigint` |
| `String`, `Bool`, `Unit`, `Never`, `Unknown` | `string`, `boolean`, `undefined`, `never`, `unknown` |
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

`@ts` (a foreign type-form attribute declared by `flick/js` in
`descriptor.type-form-attributes`; other backends declare their own, such as
`@go` on `flick/go`) before the declaration of a type or a service says what
the type is in TypeScript. Such a type is not declared: every module that names
it writes the form, and imports only what the forms it wrote name.

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
| `@ts "form"` | the form: `{i}` is the type's argument `i` (from 0), `{i:row}` that argument as a row of services or errors |
| `@ts import "module" "Export"` | a `$Export` of the form is that export of the module, named as `foreign … from` names one: a package as written, a file by its path from the source of the module that declares the type |
| `@ts none` | the type has no form: an export whose type names it is `FLICK-F004` under `--declarations` |

A form is the text of a type and nothing else: names, `.`, `|`, `&`, `?`,
`:`, `=>` and the brackets `<…>`, `(…)`, `[…]`, with a `,` inside brackets
only. A string literal and an object type are not forms; name such a type
in a declaration file and import it. A form that does not read, a place past
the parameters of the type, a `$Export` without its import, and two forms
for one type are `FLICK-S014`. The compiler does not check that the form is
true of the values: that is the word of the package, as the signature of a
`foreign` is.

The `.d.ts`, `.d.mts` and `.d.cts` files of a library ship beside its
modules, as its `.mjs` and `.cjs` do. A form is the same whichever runtime a
build is for. The built-in types have no `@ts`: the backend gives each its
form by lang item, `ReadonlyArray<A>` for `List a` and the runtime's own type
for `Option a`, as another backend gives them its representation.

`--declarations` (or `[target.js] declarations = true`) writes `A/B.d.ts` beside each module: the
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

`--promise-facade` (or `[target.js] promise-facade = true`) writes `A/B.promise.js` and
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
stays `any` in `--typescript`, and `--declarations` reports it. Code inside a module
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
| projections | a projection is a pure function of one parameter that reads a path of its fields, at most 8 deep: `.address.city` and `fun u -> u.address.city` alike. Applied, it is its reads (`fetch () \|> .address.city` calls `fetch` once, then reads two fields), whether written there, bound to a local, a top-level value of the module or exported by another module of the build, by the projection summary that module exports for it (a versioned path of labels with its types; without one the use stays a call). `List.map` of one, the built-in `List.map` and no other function of that name, is a loop that pushes the reads of each element: no function is called per element. Each field is read once per element, as the callback would; a longer path, or any function that computes, keeps the budgets of inlining |
| known shapes | a function of a built-in module that chooses what it gives with a `match`, bigger than its call, is inlined only where that spares an object: given a function written there that it calls once, which is then called where it is written; given a constructor, or a choice between constructors, that it takes apart; or when its result is taken apart at once. `a \|> f x` is inlined as `f x a` is, with `a` evaluated first. A `match` of a local bound to a constructor reads the constructor, and a `match` of a choice (an `if`, a `match`, a block) every branch of which ends in a constructor moves into the branches, an arm more than one branch takes being copied only when small; the parts of a constructor are named first, so they are evaluated where they were. `value \|> Option.filter p \|> Option.map f \|> Option.with_default 0` with `p` and `f` written there is two tests: no function, no `Some`. An arm that only names what is taken apart, itself a name or a literal (`\| n when n % 3 == 0 -> …` of `match n`), reads it by the name it has |
| propagation | a local bound to a literal or a name is replaced by it, and so is a top-level value of the module bound to a literal; a pure value that cannot throw moves to its one use when that use surely runs; a function applied to names but not yet to all its arguments, which only builds a closure, moves into the function that completes the call, so `List.map f >> List.filter p` calls both with all their arguments |
| fusion | `Seq.map`, `Seq.filter`, `Seq.flat_map` and `Seq.take` of a lazy `for`, and `List.map`, `List.filter` of a strict one, run in its loop; `Seq.to_list` of a lazy `for` is the strict one, a loop pushing into an array; a `for` over a `for` nests the inner clauses. Their functions are pure, so running them between the clauses is not observable; a known lambda is applied in place, another function is bound once before the loop. A `for` whose body makes Sync calls stays as it is. A pipeline of a `for`, a filter and a map to a list is one loop: 17× faster than the generators of debug (`tools/fusion-bench`) |
| scalar replacement | a local record whose every use reads a field, or a local tuple only destructured, becomes one local per part: no object |
| dead code | unused pure bindings (an unused `task { … }` included) and private top-level values nothing reaches; a runtime call always stays, its result used or not, but the making of a lazy cell nothing reads (below) |
| lazy cells | a lazy nothing reads is not made: `ignore (lazy { … })`, or one an inlined function takes and drops; `lazy { v }` of a value (a literal, a name, a constructor, a function, or a tuple, list or record of them) is a cell made done, whose `await` runs nothing; `await (lazy { … })` is `await (task { … })`, which runs its task there, once, and `await (lazy { v })` is `v`; a pure `lazy { e }` whose free variables are loop-invariant is hoisted out of a loop so iterations share one cell instead of making one per iteration; a `let l = lazy { … }` that never escapes its creating fiber/scope (never entering a `task`, closure, local function, lazy `Seq`, handler arm, data field, `Ref`/mutable, return value or non-inlined call) uses an unsynchronized local cell (`lazy_make_local`) without waiter tracking or locks. Each of the core rewrites is a contextual equivalence proved in Lean (`formal/Formal/Equiv/`). After `let a = await l`, a later `await l` is `a`: the cell is done with what the first gave, as both the core (`Equiv.o2a_all`) and the protocol (`formal/Formal/RT/LazyCell.lean`) prove |
| workers | a top-level function whose stages before the last are pure gets `f$n` taking all `n` arguments; a known full application calls it, a partial one keeps the curried `f`; a private function only ever applied fully has no curried form. An exported function of two or more parameters without dictionaries, `foreign` included, exports `f$n` beside `f`, and a full application in another module calls it: the `stages` of its `.iface` summary give `n` |
| dictionaries | a method of an instance without a context, written, derived or a default made its own, is an exported constant `<instance>$<method>`; a method of an instance with a context whose methods are all written as functions, and that takes no default, is the constant `<instance>$<method>` of the context's dictionaries, which its dictionary is built with. A canonical use calls it directly, in the module or an importer, given the dictionaries of the context as resolved (`$i_Show_Option_a_…$show($i_Show_Int)(x)`): no dictionary of the instance is built, which runs none of the program's code. A dictionary passed as a value (an `EqHash` witness) is never replaced |
| foreign | a pure `foreign` that passes its arguments and result as they are is the import itself: `export const div$2 = $f_div`, and a full application in the module calls `$f_div`; functions carrying a selected `@native` override lower as `foreign` bindings and are treated as opaque by the release optimizer (never inlined or constant-folded away) |
| lone effects | a runtime body that only returns one effect, `Effect.gen(function* () { return yield* x; })`, is `Effect.suspend(() => x)`: built as late, without a generator |
| matches and constructors | a `match` goes through the decision graph the go target uses too (see below), each test an `if`; the last test before a failure is not made, since the match is exhaustive; a tagged constructor applied to all its fields is the object it builds, `{ _tag: "Rect#…", _0: 3, _1: 4 }` |

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

`[build] target = "go"` (or `--target go`) builds an app into one executable without
Node. The official backend component `flick/go` writes, for each entry, a Go
module under `go/<entry>/` of the output, `package main` with one file per
Flick module and the runtime package `flickrt` beside it, and declares a
host step that compiles it with `go build` into `flick-<entry>`. The step
runs on a copy that holds only what this build wrote, so a file an earlier
build left in `go/<entry>/` never reaches the compiler. Only what `main` reaches is compiled, and a construct the
target does not compile yet is `FLICK-P012` with what it names, never a
runtime stub; for now that is a lazy `for` that reads services,
schemas deriving `Form`, and a `foreign` binding without a
Go implementation: in the runtime for the standard library, part of which
has none yet beyond the core (most of `Stream`, queues, schedules beyond
`recurs`, parts of `Codec`), or beside the program for its own bindings
(see [FFI](../ffi/#the-go-target)). Reachable uses of modules gated to other
environments (such as JS-only modules with `@when "js"`) report `FLICK-P064`
already in `flick check`, the editor and `flick build`; unused imports do not
fail. `Rpc` and `server fn` speak the
wire of the js target, so a Go backend serves a JS frontend (see
[Typed RPC](../rpc/#on-the-go-target)).

Values have the representation of the JS target, in Go terms:

| Flick | Go |
| - | - |
| `Int` (`Int32`) | `int32` |
| `Int8`, `Int16`, `Int64` | `int8`, `int16`, `int64` |
| `UInt8` (`Byte`), `UInt16`, `UInt32`, `UInt64` | `uint8`, `uint16`, `uint32`, `uint64` |
| `Float`, `Float32` | `float64`, `float32` |
| `BigInt` | `*big.Int`, never mutated |
| `String` | `string` holding WTF-8 |
| `Bool`, `Unit` | `bool`, `flickrt.Unit` |
| a tuple, a `List` | `[]flickrt.Value`; `[x, ...rest]` shares the storage of the list |
| a record (closed shape) | a dedicated Go struct `R_…` (or `R_empty` for `{}`), declared once in `record_types.go`, carried by value through known closed-type calls, locals, `let mut` variables and self tail loops. Fields lie in canonical sorted label order (JS array-index keys ascending, then UTF-16 code-unit order — matching `Record.labels` and the JS target), with concrete scalar fields (`int32`, `float64`, `bool`, `string`, fixed-width scalars) and eligible closed ADT fields stored unboxed by value, packed pointer-bearing fields first, and remaining fields as `flickrt.Value`. Code that knows the closed shape reads fields directly (`p.f0`), copies and updates `{ r with … }` by value on the stack, and compares with Go `==` or a typed `Eq_R_…` helper; field expressions still evaluate in source order. Value-receiver methods implement `flickrt.RecordView` and `flickrt.RecordEqual`, so a struct crosses into `flickrt.Value` directly without rebuilding a `*flickrt.Record` |
| an open, polymorphic, or runtime-built record | `*flickrt.Record` (or any `flickrt.RecordView` passed across the erased boundary), read by label through `flickrt.RecordView` (`Get`, `Lookup`, `Keys`, `Len`, `At`), copied by `{ r with … }`, and converted back into a closed struct via `As_R_…` at typed boundaries |
| a small, nonrecursive, closed ADT or concrete `Result` on a typed Go path | a dedicated Go value struct `A_Module__Name` (or `AI_Module__Name__<hash16>` for concrete instantiations such as `Result String Int`), declared once in `adt_types.go`, with a `uint8` discriminant (omitted for single-constructor types, up to 255 variants) and a pooled set of typed payload slots (scalars, value-free `Option`, value-free closed records, nested closed ADTs; packed pointer-bearing first, capped at 64 bytes). Carried by value with zero heap allocation through locals, `let mut`, known calls (including across modules), self tail loops, and closed aggregates; `match` tests `.tag` and reads `.f…` directly, `==` uses Go `==` or `Eq_A_…`, and verified derived `Eq`, `Ord` (`compare`, `<`, `<=`, `>`, `>=`), `Hash`, and `Show` operate directly on native fields (custom hand-written instances are never bypassed). Erased boundaries (`flickrt.Value`, polymorphic calls, closures, generic runtime helpers, FFI) convert via `Box_A_…` / `As_A_…` to and from `T_Module__Name` / `*T_Module__Name` (user ADTs) or `*flickrt.Data` (`Result`), preserving constructor identity, wire formats, codecs and `flickrt.Constructed`. Recursive, polymorphic, GADT, oversized, or `flickrt.Value`-bearing ADTs stay on the erased path (`Option` uses its dedicated typed layout below) |
| a constructor | `*flickrt.Data` with the tag of the JS target; `Fields()` is its fields, up to two of them held in the value itself |
| a tagged constructor without retained dictionaries, of a type the program declares | a Go type of its own: `*T_Module__Name` holding the fields when it has fields, or a zero-size value type `T_Module__Name` (`struct{}`) constructed as `T_Module__Name{}` with no allocation when it has none. Fields declared with a concrete scalar type (`Int`, `Float`, `Bool`, `String`) are stored unboxed (`int32`, `float64`, `bool`, `string`), packed with pointer-bearing fields first; polymorphic and non-scalar fields stay `flickrt.Value`. A `match` asks Go for the type (`Is_T_Module__Name`), compares no tag, and reads unboxed scalar fields directly without boxing; derived `Eq`, `Ord`, `Hash`, `Show`, and `==` operate directly on those typed fields. The runtime reads any constructor value as `flickrt.Constructed` (`TagName()`, `Fields()`) and builds one with `flickrt.Construct(tag, fields...)`; a constructor of a built-in module (`Ordering`, erased `Option`, …) and one holding a dictionary stay `*flickrt.Data` |
| a concrete `Option T` on a typed Go path | an allocation-free by-value Go representation chosen from `T`: `flickrt.OptBool` (`uint8` sentinel: `0` = `None`, `1` = `Some false`, `2` = `Some true`) for `Option Bool`; `flickrt.OptRef` (`struct{ val flickrt.Value }` with `nil` sentinel for `None`) when the non-`Option` payload is guaranteed non-nil (tuples, records, functions, `List`, `BigInt`, `Result`, non-unboxed ADTs); `flickrt.Opt[T]` (`{ Val T; Ok bool }` presence flag) for `Int`, `Float`, `String`, `Unknown`/opaque payloads, and every nested `Option (Option U)` so `Some None` and deeper nestings stay distinct. Polymorphic `Option a` stays tagged `*flickrt.Data`, and erased boundaries (polymorphic calls, generic runtime observers, `flickrt.Value` Go FFI) convert to and from the tagged `"Some"` / `"None"` `*flickrt.Data` form |
| a constructor declared over a record, `Node { left : Tree; key : Int }` | `*T_Module__Name` holding the closed record shape's Go struct `R_…` directly by value (`struct { rec R_… }`): one constructor allocation with unboxed scalar fields and direct `.rec.f…` access in `match`, no `*flickrt.Record` header. Labels lie in canonical sorted order (field expressions still evaluate in source order); an erased record passed whole (`Node r`) is converted via `As_R_…` |
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
use as a value makes a function of values around it. A parameter that is an `Int`, a `Float`, a `Bool`, a `String`, an eligible
concrete `Option T`, a closed record shape, or an eligible closed ADT / concrete `Result`, and a result that is one, has its typed Go representation
there (`func F(fb *flickrt.Fiber, n int32) flickrt.Opt[int32]`), so a scalar,
concrete `Option`, closed record, or closed ADT passed to a known function (including across modules), matched, unwrapped with `?`,
updated with `{ r with … }`, read via `p.field`, compared with `==`, stored in a closed aggregate or `let mut`, or carried across a self tail loop
adds no `flickrt.Value`, `*flickrt.Record`, `*T_…`, or `*flickrt.Data` allocation; adapting at the
erased `flickrt.Value` boundary happens only when crossing into open-row or polymorphic code,
closures returning `flickrt.Value`, generic runtime helpers, or `flickrt.Value`
FFI. The tail calls that are loops on the JS
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

A method written as a function is a Go function of its own, and the
instance's dictionary holds it; that of an instance with a context takes
the context's dictionaries first, as a constrained function does, and the
dictionary holds it applied to them. A method an instance without a
context computes is a package variable, set as its dictionary is built.
Where the checker resolved a method to such an instance (`show 3`,
`compare` of two values whose `Ord` is derived, `show` of an
`Option Int`), the code calls that function, or applies that variable,
with no lookup in the dictionary; the dictionaries of a context are passed
as they were resolved, a known instance, a constraint's or the local one
of an `unpack`, and the instance's own dictionary is not built. That
holds for an instance with a context only when every method is written
as a function: building its dictionary then runs none of the program's
code. The functions take and give values, not Go's own types: generic
code reaches them through the dictionary, as `List.sort` calls `compare`
of `Ord Int`, and a function of Go's types would need a wrapper
converting every argument and result there. Every other method is looked
up in its dictionary where it is used: that of a constraint, of the
argument of a parameterized module or the local one of an `unpack`, one a
value holds, one of an instance with a context that computes a method,
and a method an instance takes from a default of its class. This holds in
both profiles; in release a default may first become the instance's own
method (devirtualization above), and is then called directly too.

On both targets a `match` is compiled through a decision graph
(`flick-backend-support/src/decision.rs`). Each test the arms make of the
scrutinee, a constructor's tag, a literal or a list's length, is made once
for every arm that needs it, so arms of one constructor with different
nested patterns share its test. Tests only read the scrutinee, evaluated
once, and its parts. Arms are still tried in the order written: a guard
runs only when its arm's pattern matched and every arm before it failed,
at most once, and a guard that fails goes on to the arms after its own,
never to another alternative of its or-pattern. Names are bound before
the guard, to the parts the first alternative that matches gives. An arm
the graph reaches from several branches has its body written once: each
branch assigns the arm's names, declared before the tests, and goes to the
body with `goto`. The JS target tries such a match arm by arm instead, as it
does one of more than 16 arms, whose graph would spend the fuel budget of
the backend component, and one on a walked list. A match whose graph would have more than 32 nodes plus
16 per arm is compiled arm by arm instead.

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
toolchain, and the executable it builds needs nothing installed. The
project must also let the backend run it: `[tools]` `"flick/go" = ["go"]`
in `flick.toml`, or `--allow-tool go` for a file built outside a project
(`FLICK-P029` otherwise). It is built without cgo by default, so no C compiler is
needed either; pass `--cgo` (or `[target.go] cgo = true`) to build with `CGO_ENABLED=1`.

### Value ABI (`flick/go`)

Direct Go package bindings (`foreign name from "pkg" "Sym"`) and Go package
globals are checked against the versioned value-ABI table published by
`flick/go` (`FLICK-F013` for unsupported types, `FLICK-F014` for ABI version
mismatches). Sibling `.go` files and `from inline` bindings keep the
`flickrt.Value` escape hatch for types outside the table. Rendered from the
`flick/go` backend descriptor:

<!-- value-abi:flick/go:en:start -->
**ABI:** `flick.go-ffi@0.1.0` · **Opaque fallback:** `flickrt.Value` · **Host `catch` type:** `error` (lang item `string`)

| Key | Flick | Host | Conversion | Direction | Notes |
| - | - | - | - | - | - |
| `unit` | `Unit` | `()` | `none` | `both` | No Go parameter with `arity 0`, or no Go return value (bare `error` with `catch`); a successful Flick result is `()` |
| `bool` | `Bool` | `bool` | `none` | `both` | Go `bool` primitive |
| `int` | `Int` | `int32` | `none` | `both` | Go signed 32-bit integer `int32` |
| `float` | `Float` | `float64` | `none` | `both` | Go IEEE 754 binary64 `float64` |
| `string` | `String` | `string` | `none` | `both` | Go `string` value |
| `bytes` | `Bytes` | `[]byte` | `copy` | `both` | Freshly copied Go `[]byte` slice; host mutations never alias immutable Flick `Bytes` |
| `list` | `List a` | `[]T` | `copy` | `both` | Freshly copied Go slice `[]T` with recursive element conversion; host mutations never alias Flick `List` |
| `option` | `Option a` | `*T or (T, bool)` | `copy` | `both` | Go pointer `*T` when `nil` unambiguously represents `None` (parameters, slice elements, and `nullable` returns); presence flag `(T, bool)` / `struct{ Val T; Ok bool }` for comma-ok returns, nested `Option`, and opaque payloads; trailing `error` maps via `catch` and `Async` passes a leading `context.Context` cancelled on interruption |

Records and user-defined ADTs have no stable layout and are not in this table; direct bindings refuse them unless wrapped in an opaque nominal handle.
<!-- value-abi:flick/go:en:end -->

## Portable IR (0.17.0)

Every backend component receives a versioned `flick_ir::Program` (`ir_version = "0.17.0"`),
checked against the `ir` version range (`[0.17.0, 0.18.0)`) in its descriptor.
Alongside values, stages, dictionaries, lang items, inline foreign preambles and
bindings (`Module.preambles`, `Binding::Inline`), backend-declared foreign
adapter words (`Foreign.adapter.{words, owned}`), and fixed-width numeric types
(`Int8`, `Int16`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `Float32`), IR `0.17.0` carries the
layout-relevant types the checker inferred so layout-aware and monomorphizing
backends can lay out values by value without calling back into the checker:

| IR structure | Type information carried |
| - | - |
| `IrType` (`TypeRef`) | named type applied to arguments (`Named`), type variable (`Var`), higher-kinded application (`App`), staged function (`Fun`), tuple (`Tuple`), closed or open record row (`Record`), higher-rank scheme (`Forall`), and packed-module existential (`Exists`); effect, service and region rows are omitted |
| `Module.types` (`TypeDecl`) | each data type, `type error` and `opaque type alias` declared in the module, its layout type parameters, and each `ConstructorDecl`'s `type_params`, `existentials`, dictionary `constraints`, `fields`, `record` labels (for record-over-constructor declarations) and GADT `result_args` |
| `Constructor` | constructor `fields` (declared on `ItemKind::Constructor`, instantiated at `ExprKind::Con`) and instantiation `type_args`, alongside `dicts` and `rep` |
| Binders and expressions | `Expr.ty`, `Param.{ty, binder_types}`, `ItemKind::Value.{type_params, dict_constraints, ty}`, `ItemKind::Destructure.binder_types`, `Stmt::Let.binder_types`, `Stmt::Var.ty`, `Stmt::Fun.{type_params, dict_constraints, ty}`, `Arm.binder_types`, `Each.binder_types` and `Foreign.{type_params, ty}` |
| Polymorphic uses | `type_args` on `ExprKind::Global`, `ExprKind::Method` and `ExprKind::Con`, next to `ExprKind::Dicts` |

In memory, types are shared behind `Arc<IrType>` (`TypeRef`). On the CBOR and
JSON wire (`flick build --emit-ir`), `Program` deduplicates distinct `IrType`
nodes into a top-level `types` pool referenced by integer index so repeated
types do not duplicate the IR.

After any IR transform — including portable optimization passes and `ir` plugin
hooks — `flick_ir::validate` checks that every `IrType` is well formed, that
type arguments match the parameters of their declaration, and that type
annotations used for layout agree with the terms they describe (constructor
operands, binders, calls and results). Backends and transforms may still use an
erased representation where optional annotations are omitted (`None` or empty
lists) and the erased path stays valid. The official `flick/js` backend ignores the new compound type fields beyond bare
type-constructor names; `flick/go` reads compound `Option T`, closed record, and
closed ADT / concrete `Result` types together with `Module.types` declarations to
choose allocation-free by-value Go layouts without calling back into the checker.

## Shared data layouts

Layout-aware backends (C, WebAssembly in linear memory, LLVM) share one
deterministic layout computation in `flick-backend-support/src/layout/`
(`LayoutEngine` and `compute_layout`), alongside the shared `match` decision
graph in `flick-backend-support/src/decision.rs`. Layouts are internal to
backends and stay out of the value-ABI tables; the Go backend reads field types
without using byte sizes.

### Primitive table

A backend supplies a `PrimitiveTable` giving each primitive or handle's
`PrimitiveLayout { size, align, niche }`, where `align` is a non-zero power of
two, `size` is a multiple of `align`, and an optional `Niche { offset, width,
start, end }` names a contiguous range of unused unsigned integer bit patterns
(`width` in `{1, 2, 4, 8}` bytes) that valid values of the primitive never
hold:

| Entry | Role in layout computation | `target_32()` preset | `target_64()` preset |
| - | - | - | - |
| `unit` | `Unit` | size `0`, align `1`, no niche | size `0`, align `1`, no niche |
| `bool` | `Bool` (valid `0` or `1`) | size `1`, align `1`, niche `2..=255` | size `1`, align `1`, niche `2..=255` |
| `int`, `float` | 32-bit signed `Int`, IEEE binary64 `Float` | `4/4`, `8/8`, no niche | `4/4`, `8/8`, no niche |
| `int8`, `int16`, `int64`, `uint8`, `uint16`, `uint32`, `uint64`, `float32` | fixed-width numeric types | natural width/align, no niche | natural width/align, no niche |
| `pointer` | non-null pointer (`List`, opaque nominal handles, indirected recursive fields, erased existentials, GADT dictionaries) | size `4`, align `4`, null niche `0..=0` | size `8`, align `8`, null niche `0..=0` |
| `closure` | function values (`IrType::Fun`, higher-rank `Forall`) | size `8`, align `4`, null niche `0..=0` at offset `0` | size `16`, align `8`, null niche `0..=0` at offset `0` |
| `string`, `bigint`, `bytes` | runtime handles of `String`, `BigInt` and `Bytes` | size `4`, align `4`, null niche `0..=0` | size `8`, align `8`, null niche `0..=0` |

### Layout rules

Given a `Program` (its `Module.types` declarations and `Program.lang` table), a
validated `PrimitiveTable`, and a monomorphic `TypeRef`, `LayoutEngine::layout`
computes a `TypeLayout` containing `size`, `align`, remaining `niche`, and
`repr`:

- **Single-constructor types and `unboxed type`:** a data or error type with
  one active constructor has `Discriminant::None`. An `unboxed type` has the
  exact size, alignment and niche of its single field.
- **Multi-constructor types, `untagged type` and literal enums:** `untagged
  type` (`Rep::Typeof`) and literal enums (`Rep::Literal`) are JS forms;
  layout-aware backends give their constructors discriminants so constructors
  remain distinguishable. Payload-free constructors carry no fields and are
  encoded as discriminant values only (`Ordering` is 1 byte with tags `0`, `1`,
  `2` and remaining niche `3..=255`).
- **Niche encoding:** when a multi-constructor type has one constructor with
  non-zero payload size and that payload exposes a `Niche` with enough free bit
  patterns for all remaining (zero-payload) constructors, the discriminant is
  `Discriminant::Niche`: for example, `Option (List Int)` uses the null niche
  of the `List` pointer (`None = 0`, `Some = non-null pointer`, size `4` on
  32-bit and `8` on 64-bit), and `Option Bool` uses `2` in the `Bool` byte with
  remaining niche `3..=255`. Otherwise the type uses an explicit minimal-width
  unsigned tag (`1`, `2`, `4` or `8` bytes) at offset `0`, exposing unused tag
  values above the constructor count as a niche for enclosing enums.
- **Records and tuples:** closed records return fields in canonical
  `Record.labels` UTF-16 order and tuples return items in positional order.
  Physical byte offsets are packed deterministically by descending alignment,
  descending size, and canonical index (filling earlier alignment gaps) to
  reduce padding, since field order in memory is not observable.
- **Finite indirection analysis:** pointer indirection is decided before
  recursively expanding layouts. First, a finite declaration-level pass finds
  strongly connected components of type declarations and marks fields with
  growing recursive instantiations (such as `Nest (Nested (List a))` in
  `type Nested a = Flat a | Nest (Nested (List a))`) as `FieldStorage::Pointer`,
  leaving non-growing fields like `Flat a` by value. Second, Tarjan's strongly
  connected components over the remaining finite graph of instantiated by-value
  nominal types mark cycle-forming constructor fields (including mutual
  recursion across modules) as `FieldStorage::Pointer`. Every recursive type
  gets a finite size, non-recursive types get no unnecessary pointers, and the
  result is deterministic across module and query order.
