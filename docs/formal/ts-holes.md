# TypeScript holes and how Flick closes them

This catalog lists the known holes of TypeScript: places where TS accepts a program but the
value at run time does not match its type. For each hole it records what closes it in Flick.

Each TS example was checked on TypeScript 5.9.3 with `--strict`: it compiles, and on Node it
gives the stated result.

References:

- `core.md`, `regions.md` — the model;
- `spec/04` — `docs/src/content/docs/spec/06-typescript-and-ffi.md`;
- `reference/ffi` — `docs/src/content/docs/reference/ffi.md`.

The importer works in two steps:

1. `tools/dts-importer/src/` translates `.d.ts` into shapes (`Shape.fl`);
2. `flick-core/src/ffi/` translates shapes into Flick bindings (`convert.rs` `ty`, `import.rs` `generate`).

## 0. How a hole is closed

| Class | Meaning | Evidence |
| - | - | - |
| core | the Flick equivalent is not derivable by the rules of the model | a rule of `core.md`/`regions.md` and a negative fixture in `tests/check` |
| boundary | the TS form does not reach Flick as is: the importer rejects it or gives an opaque type, the codegen copies the data | a function of the importer or the codegen, an importer test or a run test |
| hypothesis | it rests on a trusted binding | part of hypothesis G1 (`core.md` §4) |

One hole can be closed in two classes. Then the core rejects its Flick equivalent, and the
form from `.d.ts` stops at the boundary.

Fixtures:

- `tests/check/fail/types/ts_holes.fl` — one form per definition; below it is referred to by the name of the definition.
- `tests/check/projects/ts_holes_errors` — errors with the same name in two modules.
- `tests/ffi/ts-holes` — a `.d.ts` package with forms for the importer. Its tests: `tools/dts-importer/test/import.test.mjs` and `flick-core/src/ffi/tests.rs`.
- The hole generator (`flick-model/src/generate/holes.rs`, §4) — the forms of the core's holes in the programs of the model properties (`just model`), and the changes that would open them; both checkers judge each one.

## 1. The boundary rule: a foreign object is opaque

An object, an interface or a class instance from `.d.ts` is imported as an opaque type, not as a Flick record:

- A named shape becomes `type Name` without constructors or fields: "A JS `Name`, opaque to Flick" (`convert.rs` `ty`, `import.rs` `generate`).
- An anonymous shape in a result becomes `JsValue`. In a parameter the importer answers "needs a checked adapter".
- A class export is rejected: the constructor and the methods are bound by hand.

The importer does not read the members of an object: `Shape.fl` `object_shape` reduces a class
and an object to a name. A property is accessed only through an explicit `foreign` binding with
`this`, that is, a call with its own stage, not the read of a record field. Flick gets data out of
a foreign object in two steps:

1. `Js.snapshot : JsValue ->{; SnapshotError} Unknown` copies the graph and rejects accessors, functions, foreign prototypes and cycles (spec/06 "`Unknown` and `JsValue`");
2. `Schema.validate` checks the resulting value.

spec/06 states it this way: "A simple TS object shape stays by default a foreign shape with runtime access until a checked snapshot/adapter; it does not become an immutable source record because of `readonly`".

An opaque type has no fields, so Flick sees no spread, no accessors, no `readonly` on members and
no index signatures. This closes holes §2.9–2.11 and §2.13 at the boundary.

Flick's own data (`List`, tuple, record, ADT) does not become opaque. Foreign code outside the
runtime receives and returns it as a copy ([§3.1](#31-flick-data-is-copied-at-the-foreign-boundary)):
so when JS changes an array or an object, the Flick value stays the same.

## 2. Catalog

### 2.1 `any`

```ts
const n: any = "42";
const m: number = n;
m.toFixed(1); // TypeError: m.toFixed is not a function
```

- Flick has neither `any` nor a top type: ≤ has only ≤-Refl, ≤-Fun and ≤-Eff (`core.md` §2.2).
- A foreign value of unknown type is the opaque `JsValue`. It can only be passed to foreign code or to `Js.snapshot`.
- `any` and `unknown` from `.d.ts` become `JsValue` (`convert.rs` `ty`). With `strict = true` the importer rejects them.
- Closed: core and boundary.
- Tests:
  - `ts_holes.fl` `any_as_string` — `FLICK-T003` "expected `String`, found `JsValue`";
  - `ffi/tests.rs` `any_pure_and_cancellation_are_never_implied` and `strict_mode_refuses_unknown`.

### 2.2 `as T`

```ts
type Cat = { name: string; meow(): string };
const c = { name: "x" } as Cat;
c.meow(); // TypeError: c.meow is not a function
```

- Flick has no cast.
- `(e : σ)` is `let x : σ = e in x` (`core.md` §1.4). It is typed by (Let-Sig) and (Sub). An annotation can widen the stage of an arrow or the row of an `Eff`, but cannot change the type.
- Closed: core (§2.2–2.3).
- Test: `ts_holes.fl` `cast` — `(1 : String)` gives `FLICK-T003`.

### 2.3 `as unknown as T`

```ts
const n = "42" as unknown as number;
n.toFixed(1); // TypeError: n.toFixed is not a function
```

- `Unknown` is not a top type in Flick: by (Sub) no value goes into it.
- A value of `Unknown` is created only by `Js.snapshot`. The only way out of it is `Schema.validate : Schema a -> Unknown -> Result SchemaError a`, that is, with a check at run time.
- Closed: core.
- Test: `ts_holes.fl` `double_cast` — `FLICK-T003` on both annotations.

### 2.4 `!`

```ts
const m = new Map<string, number>();
const v: number = m.get("k")!;
v.toFixed(1); // TypeError: Cannot read properties of undefined
```

- Flick has no `null` or `undefined`: an absent value is `Option`.
- An `Option` is taken apart only by `match`, which must be exhaustive (Exh, `core.md` §2.4).
- A refutable `let Some n = v` is `FLICK-T009`.
- `let assert Some n = v` on a mismatch is an explicit defect. This is the outcome `die`, which T1 allows, not a value of another type.
- Closed: core.
- Test: `tests/check/fail/types/matching.fl` `refutable` (`FLICK-T009`).

### 2.5 A lying predicate `x is T`

```ts
function isNumber(x: unknown): x is number { return true; }
const s: unknown = "a";
if (isNumber(s)) s.toFixed(1); // TypeError: s.toFixed is not a function
```

- `if` and `match` on a `Bool` value do not change the type of a variable.
- Only a constructor pattern changes a type. The type of the bound variable comes from the constructor's declaration (`core.md` §2.4), not from a function's promise.
- The importer does not read a predicate from `.d.ts`. For TS the return type `x is T` is `boolean`, and the binding returns `Bool`. `asserts x is T` gives `Unit`.
- Closed: core and boundary.
- Tests:
  - `ts_holes.fl` `guard` — after `if is_point value` the field `value.name` gives `FLICK-T003`;
  - the importer test "reduces a type predicate to its boolean".

### 2.6 Bivariant method parameters

```ts
class Ev { constructor(readonly type: string) {} }
class MouseEv extends Ev { constructor(readonly clientX: number) { super("mouse"); } }
interface Handler { handle(e: Ev): string }
const h: Handler = { handle(e: MouseEv) { return e.clientX.toFixed(1); } };
h.handle(new Ev("key")); // TypeError: Cannot read properties of undefined
```

- In Flick only arrows and `Eff` have subtypes. An arrow's parameter is contravariant (≤-Fun, `core.md` §2.2; `effects.rs` `subsume`).
- Records have no methods that would be compared differently. An instance method is checked against the class signature as written (`FLICK-C010`).
- The Flick analogue: a function that relies on a pure callback is put where it will be passed an effectful one.
- Closed: core.
- Test: `ts_holes.fl` `pure_callback` — `(Unit -> Unit) ->{} Unit` in place of `(Unit ->{} Unit) ->{} Unit` gives `FLICK-T003`.

### 2.7 Covariant mutable arrays

```ts
class Animal { constructor(readonly name: string) {} }
class Dog extends Animal { bark() { return "woof"; } }
class Kitten extends Animal { meow() { return "meow"; } }
const cats: Kitten[] = [];
const animals: Animal[] = cats;
animals.push(new Dog("rex"));
cats[0].meow(); // TypeError: cats[0].meow is not a function
```

- `List` is immutable. The only mutable cell is `Ref`, and `Ref` is invariant: inside `Ref`, `List`, records and ADTs types are compared by ≡ (`core.md` §2.2, [only arrows and Eff have subtyping](core.md#only-arrows-and-eff-have-subtyping)).
- At the boundary `T[]` and `readonly T[]` from `.d.ts` become `List T`. An array that crosses the boundary is copied ([§3.1](#31-flick-data-is-copied-at-the-foreign-boundary)).
- Closed: core and boundary.
- Tests:
  - `ts_holes.fl` `covariant_ref` — `Ref (Unit -> Unit)` in place of `Ref (Unit ->{} Unit)` gives `FLICK-T003`;
  - the importer test "gives a readonly array the shape of a mutable one";
  - the run test of [§3.1](#31-flick-data-is-copied-at-the-foreign-boundary).

### 2.8 Width subtyping

```ts
type Point2 = { x: number; y: number };
const p3 = { x: 1, y: 2, z: "far" };
const p2: Point2 = p3; // excess properties are checked only on a literal
const total: number = Object.values(p2).reduce((s, v) => s + v, 0);
total; // "3far"
```

- Flick records are exact. (Rec) gives a record with exactly the given fields, and there is no inclusion of field rows (`core.md` §1.2: "no width subtyping").
- An extra field is `FLICK-T003`, both in a literal and through a variable.
- An open tail `{ name : String | r }` is polymorphism, not a subtype: the function gets the whole record, at its own type.
- Closed: core.
- Tests:
  - `ts_holes.fl` `width_via_variable` — through a variable;
  - `tests/check/fail/types/records.fl` `extra` — in a literal.

### 2.9 Spread

```ts
class Temperature { #c = 20; get celsius() { return this.#c; } }
interface Temp { readonly celsius: number } // how `.d.ts` describes a getter
const t: Temp = new Temperature();
const copy = { ...t };          // type { celsius: number }
copy.celsius.toFixed(1);        // TypeError: Cannot read properties of undefined
```

The type of `{...a}` is the same for an enumerable own property, a non-enumerable one and an
accessor on the prototype. Only the first gets a value, and the prototype is lost.

- Flick has no spread. `{ r with ℓ = e }` exists only for Flick records, and those have only own data fields (spec/06 "Value representation": "a plain JS object with the exact set of own data fields").
- A foreign object is an opaque type (§1). Neither `with` nor a field read applies to it.
- Closed: boundary.
- Tests:
  - `ts_holes.fl` `spread_handle` — `FLICK-T003` "`Handle` has no field `name`; `with` only replaces existing fields";
  - the importer test "reads no members of a class";
  - `ffi/tests.rs` `an_object_type_is_opaque_without_fields`.

### 2.10 A setter-only property

```ts
class Box { v = 0; set value(x: number) { this.v = x; } }
const x: number = new Box().value;
x.toFixed(1); // TypeError: Cannot read properties of undefined
```

- Flick has no access to the properties of a foreign object (§1). A binding with `this` is a call of a function a person wrote, not the read of an accessor.
- `Js.snapshot` rejects accessors.
- Closed: boundary.
- Tests:
  - `ts_holes.fl` `field_of_handle` — `FLICK-T003` "`Handle` has no field `value`";
  - the importer test "reads no members of a class": a class with a getter, a setter and an index signature gives only `handle`.

### 2.11 `readonly` and another reference

```ts
const frozen: { readonly n: number } = Object.freeze({ n: 1 });
const loose: { n: number } = frozen; // readonly does not take part in compatibility
loose.n = 2; // TypeError: Cannot assign to read only property 'n'
```

- Flick data is immutable. Only `Ref` is mutable, and that shows in the type.
- There is no read-only view of a `Ref`. The core has no form that would have to be rejected, so there is no negative fixture here.
- Foreign code receives Flick data as a copy ([§3.1](#31-flick-data-is-copied-at-the-foreign-boundary)), so another reference from JS does not change it either.
- Closed: core and boundary.
- Test: the run test of [§3.1](#31-flick-data-is-copied-at-the-foreign-boundary).

### 2.12 Optional property versus `| undefined`

```ts
type Opts = { retries?: number };
const o: Opts = { retries: undefined }; // without exactOptionalPropertyTypes
"retries" in o ? o.retries!.toFixed(0) : "none"; // TypeError
```

- Flick has no `undefined`. A field of type `Option T` is required: omitting it is `FLICK-T003`. The distinction between "no key" and "no value" does not arise.
- At the boundary both a parameter `x?: T` and a parameter `x: T | undefined` become `Option T` marked `optional i`. `None` passes an explicit `undefined` at the same arity (`runtime/src/core/data.ts` `optionalArg`). A function that tells a missing argument from `undefined` (`arguments.length`, `in`) sees the same thing for both. This is hypothesis G1.
- A result `T | undefined` is imported only under the policy `undefined = "none"`.
- Closed: core; at the boundary, hypothesis.
- Test: `tests/check/fail/ui/children.fl` `panel_no_header` — a missing field of type `Option`.

### 2.13 An index signature returns `T`

```ts
const scores: Record<string, number> = {};
const s: number = scores["ann"]; // without noUncheckedIndexedAccess
s.toFixed(1); // TypeError: Cannot read properties of undefined
```

- A lookup by key or by index returns `Option`: `HashMap.get`, `List.get`, `Record.get_key`, `String.at`. These are the types of constants of Γ₀.
- The importer does not read index signatures. `Record<K, V>` is outside the supported subset.
- Closed: core and boundary.
- Tests:
  - `ts_holes.fl` `index_lookup` — `FLICK-T003` "expected `Int`, found `Option Int`";
  - the importer test "leaves an index signature outside the supported subset".

### 2.14 Structurally equal errors with the same `_tag`

```ts
class StoreMissing { readonly _tag = "NotFound"; constructor(readonly key: string) {} }
class CacheMissing { readonly _tag = "NotFound"; constructor(readonly key: string) {} }
function fromStore(): StoreMissing { return new CacheMissing("k"); }
fromStore() instanceof StoreMissing; // false
```

In TS only a hand-written brand can tell them apart: a field
`declare protected readonly brand: "StoreMissing"` unique to the class.

- An element of an error row is identified by its name `Module.Name` (`core.md` §1.2).
- The constructors of two modules are different values. Their `_tag` is the name plus 48 bits of FNV-1a of the ids of the package, module, type and constructor (spec/06 "Value representation").
- An arm `A.NotFound _` removes only `App.A.NotFound` from the row.
- Closed: core.
- Tests:
  - `tests/check/projects/ts_holes_errors` — `FLICK-E002`: `App.B.NotFound` stays in the row;
  - `flick-core/src/codegen/tests.rs` `constructor_ids_name_package_module_and_type` — tags differ between modules and packages.

## 3. Findings

Each of the first three findings was confirmed by a probe: the program was accepted, and its
value at run time disagreed with its type. The fourth was found by the hole generator (§4).
All four are fixed.

### 3.1 Flick data is copied at the foreign boundary

The finding: Flick data crossed the foreign boundary by reference.

- spec/06 "Value representation" promises: "Immutable data is copied by default at an untrusted input and output boundary".
- The codegen passed `List`, tuples, records and ADTs as they are, in both directions (`backends/js/src/codegen/js/foreign.rs` `foreign_call`).
- The probe is `tests/run/ffi/copy.fl` on Node:

  ```text
  argument [1, 2, 3], result [1, 2, 3]     JS sorted the argument in place
  held result [1, 2, 99]                   JS changed an array it had returned earlier
  record mallory ["a", "b"]                JS changed a record
  callback result [1, 2, 9], JS saw 3      JS changed what a callback returned
  ```

  An immutable Flick value changes. In the same way JS can put a string into a `List Float`, which breaks T1.
- The fix is in the codegen, as spec/06 promises.
  - The untrusted boundary is every `foreign` outside `@flick-lang/runtime`, including `global`.
  - Copied are the arguments (and `this`), the result (for `async` and `effect`, the success value), and the arguments and result of a callback.
  - The type decides what to copy: a position with a `List`, a tuple, a record, an ADT or a type variable (`types/ffi.rs` `holds_data`, the plan in `ForeignInfo.copies`).
  - Primitives, `JsValue`, opaque types and functions are passed as they are. A callback without data keeps its identity.
  - The copy is made by the runtime's `copyData` (`runtime/src/core/data.ts`; on Effect, `packages/effect-ts/src/data.ts`, with Effect's `Option` and `Result`). It copies arrays and plain objects all the way down and passes other objects as they are.
  - The trusted boundary (the runtime) does not copy. For it this is hypothesis G1.
- Test: `tests/run/ffi/copy.fl` prints `argument [3, 1, 2], result [1, 2, 3]`, `held result [1, 2]`, `record ann ["a"]`, `callback result [1, 2], JS saw 3`.

### 3.2 `typeof C` is not imported as `C`

The finding: `typeof C` was imported as `C` itself.

- `Shape.fl` `object_shape` gave `handle` to any type with a class symbol. The static side of a class, that is its constructor, got the same name as an instance.
- The probe: `register(ctor: typeof Point): void` became `register : Point ->{} Unit`, and `register (make_point 1)` passed the check. JS received an instance instead of the class.
- The fix is in the importer: a type with construct signatures and a class symbol is `other`, that is, a refusal "outside the supported subset".
- Test: the importer test "keeps the static side of a class apart from its instances".

### 3.3 Two TS types with one simple name block their exports

The finding: two TS types with the same simple name were one Flick type.

- In a generated module opaque types differ only by their simple name (`import.rs` `generate`, the set `handles`).
- The probe: `retry(options: a.Options)` and `logOptions(): b.Options` gave one `type Options`, and `retry (log_options ())` passed the check.
- The fix is in the importer. It finds simple names that stand for different declarations and blocks the exports that name them. Bindings for those are written by hand, with different names.
  - `Shape.fl` marks a named type with its place of declaration (`site`) and removes the mark before the fingerprint and the output, so fingerprints and the shapes of exports do not change.
  - `Import.fl` `ambiguous` collects the names with different places into the surface field `ambiguous`.
  - `import.rs` `generate` blocks an export that names such a name: "`Options` names more than one TypeScript type".
- Tests: the importer test "names the object types whose one name stands for two declarations", `ffi/tests.rs` `a_name_of_two_typescript_types_blocks_what_uses_it` and `an_object_type_is_opaque_without_fields`.

### 3.4 A warning in an arm no longer disables the exhaustiveness check

The finding: a warning in an arm disabled the exhaustiveness check of `match` (hole 2.4).

```flick
warned : Option Bool -> Int
let warned o =
  match o with
  | None ->
    match 1 with
    | _ -> 0
    | 2 -> 1
```

- What happened: the program was accepted without errors although the arm `Some _` is missing; on `Some true` the `match` finds no arm. The inner `match` gives only the warning `FLICK-T010`.
- The cause: `infer.rs` `match_expr` checked exhaustiveness only if the arms added no diagnostics at all, warnings included. The check is skipped after any error in the arms so as not to report patterns of the wrong type, but a warning does not spoil the type.
- The fix is in the checker: the check is skipped only after an error. The model (`core.md` §2.4, Exh) always rejected the program; the divergence was found by the property `checkers_agree` on a program with a `match` from which the generator removed an arm.
- Fixture: `tests/check/fail/types/matching.fl` `warned`.

### What remains hypothesis G1

These are taken on trust, and the tests do not check them:

- a `never` result (`Never`);
- the parametricity of generics: `.d.ts` does not prove it (spec/06);
- a primitive result: `number` is not checked at run time;
- the distinction between a missing argument and `undefined` (§2.12);
- an export's fingerprint: for a named type it is only the name, so `ffi check` does not see a change in its members.

### Outside the catalog, not fixed

- `T | void` in a result requires a policy (`convert.rs` `result`), but `check.rs` `required` does not require `nullable` of a checked binding;
- a name from a package such as `Option` or `String` is declared in the generated module and shadows the prelude name. Usually this is a loud type error, not a silent one;
- an empty tuple `[]` becomes `Unit`, so JS receives `undefined`.

## 4. The hole generator

The program generator of the model properties (`flick-model/src/generate/`) builds Flick forms
whose TS counterpart has a hole, and `mutate` makes the change that would open that hole. The
property `checkers_agree` (`flick-model/tests/props.rs`) requires the model and flick-core to
accept the changed program together or reject it together. The bindings are in `Gen.fl`:
`JsValue` from `js_of`, the opaque `Shape` from `shape_of`, the member `shape_name` (a call with
`this`), the predicate `is_point` (`foreign pure`).

| Hole | Form in the program | Change |
| - | - | - |
| 2.1 `any` | `JsValue` and `Shape` among the data types | a subterm replaced by a term of another type |
| 2.2 `as T` | an annotation `(e : σ)` | `(e : σ')` with another type |
| 2.3 `as unknown as T` | — | `((e : Unknown) : σ)` |
| 2.4 `!` | `let (x, y) = e` | the `None` arm removed: `let Some x = e` |
| 2.5 predicate | `if is_point v` | a field of a `JsValue` |
| 2.6 method parameter | `∀a b. (a → b) → a → b` and higher-order functions | the stage of a parameter in the signature widened |
| 2.7 covariant array | `let c : Ref (a → b) = Ref.make (f : a → b)` | the stage under `Ref` in the signature widened |
| 2.8 width subtyping | records | an extra field in a literal |
| 2.9 spread | `{ r with a = e }` | `with` on `Shape` and `JsValue` |
| 2.10 setter | a member through a call of `shape_name` | a field of `Shape` |
| 2.12 optional property | records with `Option` fields | a field missing from a literal |
| 2.13 index signature | `List.get i [e, e']` gives `Option` | a subterm replaced by a term of another type |

2.11 and 2.14 do not fit in one generator program. 2.11 needs another reference from JS: the run
test of [§3.1](#31-flick-data-is-copied-at-the-foreign-boundary). 2.14 needs errors with one name
in two modules: the project `tests/check/projects/ts_holes_errors`.

The generator found the checker hole of [§3.4](#34-a-warning-in-an-arm-no-longer-disables-the-exhaustiveness-check),
for hole 2.4, and two incompletenesses in the regions: [Scope in function row is not a capture](regions.md#a-scope-in-a-local-function-row-is-not-a-capture) (recorded) and
[qualified meta is not bound to itself](regions.md#a-qualifier-on-a-meta-does-not-bind-the-meta-to-itself) (fixed) in `regions.md`. The abstract machine does not run programs with
foreign values: it has no natives for `Gen.fl` (`generate::runs`).
