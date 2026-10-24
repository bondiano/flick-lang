---
title: "Code generation"
---

`flick build` compiles each module to an ES module (see [TypeScript generation and the FFI boundary](../../spec/08-typescript-and-ffi/), [The Flick compiler and Content Mapper integration](../../spec/11-compiler/)). This page
describes the debug profile, which is the reference lowering: every other
profile must behave the same way.

## Output

| Path in the output directory | Contents |
| - | - |
| `A/B.js`, `A/B.js.map` | module `A.B` and its source map |
| `_flick/<Name>.js` | the built-in modules (`Prelude`, `Platform`, …) |
| `flick-main.js` | for an app: runs `main` of the entry module through the runtime's entry runner |
| `package.json` | `{ "type": "module", "sideEffects": ["./flick-main.js"] }`; `"sideEffects": false` without a runner |

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
| `Number`, `BigInt`, `String`, `Bool` | the JS primitive; number literals keep their JS spelling |
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
field of each constructor, which must differ in JS type — `Number`,
`String`, `Bool`, `BigInt`, or one list or tuple (`FLICK-T020` otherwise).
A pattern tests `typeof` or `Array.isArray` instead of `_tag`; derived
`Eq`, `Ord`, `Hash` and `Show` follow the representation; `deriving Schema`
of an unboxed type has the field's wire form, an untagged one has no
derived witness. The TypeScript projection is the field type, or the union
of the field types.

```flick
unboxed type UserId = UserId String
  deriving Eq, Show, Schema

untagged type Key = ByNumber Number | ByName String | ByPath (List String)
```

`opaque unboxed type Token = Token String` hides the constructor from
importers as `opaque type` does, and keeps the string itself at run time.

`==` is canonical structural equality: SameValueZero on numbers, so
`NaN == NaN` and `0 == -0`; `Number.strict_equal` is `===` and
`Number.same_value` is `Object.is`. `<`, `<=`, `>`, `>=` are the JS
operators; `Ord Number` (`compare`) is the total order with NaN last.
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
| `scope [s] { body }` | `$rt.scope((s) => body)`: a lexical owner with a two-phase close, see [runtime.md](../runtime/) |
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
with a constraint is a function of its dictionaries. The dictionary of
`instance Show Number` is `$i_Show_Number`; a head that is not names
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
under `node --enable-source-maps` points at `.fl` lines.

## TypeScript

`flick build --emit ts` (or `emit = "ts"` in `flick.toml`) writes the same
modules as `.ts` files, importing each other with the `.ts` extension. Each
module declares the TypeScript form of the types it exports and annotates its
exported values with their projected types (see [Public TypeScript interfaces](../../spec/08-typescript-and-ffi/#public-typescript-interfaces)):

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
Importing either file runs nothing.

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
| constant folding | literal expressions with JS semantics bit for bit (`60 * 60 * 24` → `86400`); a branch of a constant condition; a result JS might compute differently (`2 ** 0.5`) or would throw (`1n / 0n`) stays a computation |
| inlining | a known full application of a small pure function of the module, or of a private one used once; the arguments are bound in order first; beta reduction of `(fun x -> …) a` |
| propagation | a local bound to a literal or a name is replaced by it; a pure value that cannot throw moves to its one use when that use surely runs |
| scalar replacement | a local record whose every use reads a field, or a local tuple only destructured, becomes one local per part: no object |
| dead code | unused pure bindings (an unused `task { … }` included) and private top-level values nothing reaches; a runtime call always stays, its result used or not |
| workers | a top-level function whose stages before the last are pure gets `f$n` taking all `n` arguments; a known full application calls it, a partial one keeps the curried `f`; a private function only ever applied fully has no curried form. An exported function of two or more parameters without dictionaries, `foreign` included, exports `f$n` beside `f`, and a full application in another module calls it: the `stages` of its `.iface` summary give `n` |
| dictionaries | a method written in an instance without a context is an exported constant `<instance>$<method>`; a canonical use calls it directly, in the module or an importer; a dictionary passed as a value (an `EqHash` witness) is never replaced |
| foreign | a pure `foreign` that passes its arguments and result as they are is the import itself: `export { $f_div as div$2 }`, and a full application in the module calls `$f_div` |
| lone effects | a runtime body that only returns one effect, `Effect.gen(function* () { return (yield* x); })`, is `Effect.suspend(() => x)`: built as late, without a generator |

A function whose first stage computes or runs something gets no worker:
joining the stages would move that work after the next argument. Pure
self tail recursion is a loop in both profiles; a runtime recursion stays a
chain of `Effect.gen` or `Effect.suspend` steps, so it yields to the scheduler and can be
interrupted. Nothing reassociates arithmetic (`(a + b) + c`), folds `x * 0`,
runs or memoizes a task, merges two `Layer` builds, moves a service lookup
across `provide` or drops a schema check.

## Not compiled yet

`sql` and `cli` blocks are checked but not compiled: `flick build` reports
`FLICK-P012`.
