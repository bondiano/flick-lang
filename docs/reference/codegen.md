# Code generation

`flick build` compiles each module to an ES module (§8, §11). This page
describes the debug profile, which is the reference lowering: every other
profile must behave the same way.

## Output

| Path in the output directory | Contents |
| - | - |
| `A/B.js`, `A/B.js.map` | module `A.B` and its source map |
| `_flick/<Name>.js` | the built-in modules (`Prelude`, `Platform`, …) |
| `main.js` | for an app: runs `main` of the entry module through the runtime's entry runner |
| `package.json` | `{ "type": "module" }` |

Without a project, `flick build <files> --out-dir <dir>` writes the same
layout; a root file whose module exports `main` gets the runner. Importing a
module never runs anything: modules only define values.

Generated code imports `@flick-lang/runtime` only: `Effect` comes through it.
A module that has no runtime stage imports no `Effect` at all.

## Values

| Source value | JS |
| - | - |
| `Unit`, `()` | `undefined` |
| `Number`, `BigInt`, `String`, `Bool` | the JS primitive; number literals keep their JS spelling |
| tuple, `List` | dense array |
| record | plain object with exactly its fields, in the order written |
| ADT value | `{ _tag, _0, _1, … }`, fields in declaration order |
| `type error E = E …` | the same shape: errors are values of their one constructor |
| `Option`, `Result` | native Effect `Option` (`Some`/`None`, `value`) and `Result` (`Success`/`Failure`, `success`/`failure`) |
| `Ref` | a runtime cell, equal only to itself |

A constructor id names the package, module, type and constructor:
`@acme/shapes/App.Shapes.Shape.Circle`; without a package (a file outside a
project, the built-in modules) it is `App.Shapes.Shape.Circle`. A user type
with a `Some` constructor is never mistaken for `Option`.

`==` is canonical structural equality: SameValueZero on numbers, so
`NaN == NaN` and `0 == -0`; `Number.strict_equal` is `===` and
`Number.same_value` is `Object.is`. `<`, `<=`, `>`, `>=` are the JS
operators; `Ord Number` (`compare`) is the total order with NaN last.

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
  const user = (yield* $rt.operation($s_Users, "get", 1)(id));
  const name = normalize_name(user.name);
  (yield* $rt.operation($s_Log, "info", 1)(name));
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
| `scope [s] { body }` | `$rt.scope((s) => body)`: a lexical owner with a two-phase close, see [runtime.md](runtime.md) |
| a call acquiring into an outer `scope` | `$rt.within(owner, call)`, the owner of that scope's region |
| pure self tail call | a loop over the parameters |
| runtime recursion | through `yield*`, without growing the JS stack |

## Services

`service S` in module `M` is a runtime service key `M.S`. `S.op` looks the
service up at the first runtime stage of the operation and calls the method
of the implementation record; a method of an `S.Impl` value is called
directly. `Layer.succeed S impl` builds the layer.

## Dictionaries

A definition with class constraints takes their dictionaries first:
`render : Show a => a -> String` is `(dShow) => (value) => …`; a use passes
them, `render(<Show Number>)(1)`. A dictionary is an object of the class's
methods with a getter per superclass (`$Eq` of an `Ord` dictionary). An
instance with a context is a function of the context's dictionaries. Derived
instances and the compiler-owned instances of records and tuples are built
by the runtime from the dictionaries of their fields. A higher-rank argument
with a constraint is a function of its dictionaries.

## Foreign functions

`foreign mode name from "module" "export"` imports the export; the curried
Flick function calls it with all arguments of its stages up to the first
runtime one (or `arity n`), dictionaries first. `pure` is a direct call,
`sync` runs in `Effect.sync`, `async` awaits the returned `Promise` — the
innermost owner waits for it to settle even after an interruption (see
[runtime.md](runtime.md)) — and `effect` returns an `Effect`. A relative module is
resolved against the source file. `global` names a global, as in
`foreign sync now from global "Date.now"`.

## Source maps

Each module has a version 3 source map. Definitions, statements, calls,
operators and returns map to the start of their Flick node, so a stack trace
under `node --enable-source-maps` points at `.fl` lines.

## TypeScript

`flick build --emit ts` (or `emit = "ts"` in `flick.toml`) writes the same
modules as `.ts` files, importing each other with the `.ts` extension. Each
module declares the TypeScript form of the types it exports and annotates its
exported values with their projected types (§8 "Публичные TypeScript
интерфейсы"):

| Flick | TypeScript |
| - | - |
| `Number`, `String`, `Bool`, `BigInt`, `Unit`, `Never`, `Unknown` | `number`, `string`, `boolean`, `bigint`, `undefined`, `never`, `unknown` |
| `List a`, `(a, b)`, `{ x : a }` | `ReadonlyArray<A>`, `readonly [A, B]`, `{ readonly x: A }` |
| `Option a`, `Result e a` | Effect `Option.Option<A>`, `Result.Result<A, E>` |
| `a -> b` | `(arg: A) => B`, one arrow per stage |
| `a ->{r; e} b`, `Eff r e a` | `(arg: A) => Effect.Effect<B, E, R>`, `Effect.Effect<A, E, R>`; an empty row is `never` |
| a variant type, an error | a union of `{ readonly _tag: "<constructor id>"; readonly _0: … }` |
| an opaque, abstract or built-in nominal type, a service | an interface branded with its qualified name |
| a class over types | the interface of its dictionary; a constrained value takes it first |
| `forall a.` | a generic `<A>`; a polymorphic constant uses `never` |

A type TypeScript cannot state faithfully — a higher-kinded variable, a
region or a value holding one, an open record — has no projection: the export
stays `any` in `--emit ts`, and `--emit-dts` reports it. Code inside a module
is annotated `any`: the source types live in the Flick checker and its
`.iface`. The output runs as it is on Node (type stripping) and passes
`tsc --noEmit` with `strict`, `allowImportingTsExtensions` and
`module: nodenext`. `@flick-lang/runtime` ships `index.d.ts` for it.

## Not compiled yet

`sql`, `ui`, `cli`, `codec` and `schema` blocks are checked but not compiled:
`flick build` reports `FLICK-P012`.
