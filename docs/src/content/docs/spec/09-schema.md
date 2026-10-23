---
title: "9. Schema and standard data types"
---

## The boundary between the language core and the library

The core defines pure/runtime, nominal types, HKT, classes, captures and full deriving Schema. `Schema`, `Ref`, `Option`, `Result`, `Redacted`, `BigDecimal`, `Duration` and `Chunk` are library types with the native Effect ABI. In the first version only plain Number and BigInt have special literals; Schema/SQL/UI have a chosen DSL, and the other modules use ordinary functions.

| Source type | Representation in Effect and TS |
| - | - |
| `Eff r e a` | `Effect.Effect<A, E, R>` |
| `Layer ri e ro` | `Layer.Layer<RO, E, RI>` |
| `Option a` | `Option.Option<A>` |
| `Result e a` | `Result.Result<A, E>` |
| `Ref a` | `Ref.Ref<A>` |
| `Redacted a` | `Redacted.Redacted<A>` |
| `Codec rd re i a` | `Schema.Codec<A, I, RD, RE>` |
| `PureCodec i a` | The same Schema codec and a checked source guarantee of purity |
| `Schema a` | A pure domain witness based on `Schema.Codec<A, A, never, never>` |
| `BigDecimal`, `Duration` | Native values of the Effect modules of the same name |

The order of generic parameters of the source API may differ from TS; this does not require repacking the value. Source metadata preserves kinds, rows and purity guarantees that `.d.ts` does not have.

| Capability | Benefit | Cost |
| - | - | - |
| Library combinators | A simple core and explicit values | More boilerplate |
| Types and structural deriving | One source for the structure of ADTs and records | Generation must account for recursion, generics and unsupported fields |
| Refinement types with a runtime check | Domain constraints are visible in types | Safe constructors and rules for trusted FFI are needed |
| A DSL for the external format | Renames, defaults and transformations are kept together | Extra grammar and diagnostics |
| Arbitrary type-level computation | Very expressive libraries | A substantial complication of the checker and of compile time |

Deriving covers the whole supported data language. Library Schema combinators and the codec DSL remain the ways to customize and extend. This does not require introducing arbitrary type families.

## The type of a value and its external format

A source type describes the values of the program. A Schema is a runtime value that checks structure and constraints. A Codec additionally describes the conversion between the external and the internal representation. One domain type may have different codecs for HTTP, the database and configuration.

In [Effect v4](https://effect.website/docs/v4/schema/getting-started), the general codec type has four parameters: `Schema.Codec<T, Encoded, DecodingServices, EncodingServices>`. `Encoded` means the external format, not an error channel. It is useful to keep the separate decoding and encoding requirements in Flick.

Source types put the decoded result last. This does not promise a `Functor` for Codec: the encoder needs the reverse conversion, so `imap` or a separate decoder/encoder is used. `rd` and `re` denote decoding/encoding services. The expected error in both directions is `SchemaError`; a domain error of an effectful transformation is translated explicitly into a schema issue, while defect/interruption are preserved.

```text
Schema a
PureCodec encoded decoded
Codec decoding_services encoding_services encoded decoded

Codec.decode :
  forall c ci rd re i a.
  (Codec rd re i a)^c ->
  (Unknown^ci ->{rd; SchemaError} a)^c

Codec.encode :
  forall c rd re i a.
  (Codec rd re i a)^c -> (a ->{re; SchemaError} i)^c

Codec.decode_result :
  forall c ci i a.
  (PureCodec i a)^c ->
  (Unknown^ci -> Result SchemaError a)^c

Codec.encode_result :
  forall c i a.
  (PureCodec i a)^c -> (a -> Result SchemaError i)^c
```

The codec's captures are preserved on a partially applied function; the provenance of the returned `a`/`i` is checked separately and is not erased by a generic witness. SchemaError carries a safe diagnostic, not the live source input, which could carry a scoped capability out.

`PureCodec` is an opaque certificate of pure synchronous execution, not an alias over empty service rows. `Codec none none i a` may perform async/runtime actions. The certificate is created by checked pure combinators, deriving and explicitly trusted FFI. An external Effect Schema without such a contract gets the general Codec. Pure decoding accepts only a source Unknown with an inert structural representation; arbitrary JS first goes through the runtime import from section 8.

`Schema a` in this source API is also a certified pure domain witness; this is a narrower contract than an arbitrary external Schema. `PureCodec` and `Schema` use the same Effect Schema runtime object, and the extra guarantee is stored in the source interface. Effectful Schema interpreters remain available through runtime arrows. [Effect v4 Schema](https://effect.website/docs/v4/schema/getting-started).

## Types and Schema from one declaration

The chosen grammar is `refined type`, `deriving Schema`, `schema T` and `codec witness { ... }`. A refinement is a nominal type with a closed constructor, a pure predicate, a safe `parse` and access to the underlying value. The predicate returns Bool and does not normalize the value; false is a validation failure, and an unexpected defect remains a defect. Trim/lowercase/parse live in the codec transformation. `Checks.email` is a concrete library check, not a promise to validate every email address.

```flick
refined type Email = String where Checks.email
  deriving Eq, Hash, Schema

type alias Profile = {
  email : Email;
  nickname : Option String;
} deriving Eq, Hash, Schema

profile_schema : Schema Profile
let profile_schema =
  schema Profile

profile_json : PureCodec Json Profile
let profile_json =
  codec profile_schema {
    email = field "email_address" Email.json;
    nickname = field "nickname" (option_as_null String.json);
  }

parse_profile : Unknown -> Result SchemaError Profile
let parse_profile input =
  Codec.decode_result profile_json input
```

In the codec DSL every decoded field of a record is given exactly once. A wire-name conflict, an unknown field or a missing required field is a compile error. By default, extra input keys are rejected; `ignore_extra_fields` explicitly builds a checked projection without them. To preserve unknown fields, a declared field of the corresponding type is required. In the example `None` is encoded as JSON null; missing, null and a tagged Option are not chosen implicitly from `Option a` alone. [Effect Schema representations](https://effect.website/docs/v4/schema/effect-data-types).

### Option and default policies

A field's policy is set by the codec DSL; it cannot be derived from `Option a` alone.

| Policy | Encode None | Decode |
| - | - | - |
| `field "x" codec` | By the codec | The key is required |
| `field "x" (option_as_null codec)` | null | null means None; missing is an error |
| `optional_field "x" codec` | The key is absent | missing means None; a present value is checked by the codec |
| `field "x" (option_as_nullish codec)` | Canonical null | missing/null collapse to None; undefined only in an explicitly chosen JS format |
| Tagged Option codec | An explicit None branch | A checked discriminant and payload |

`undefined` is not part of `Json`. If missing and null must be distinguished, a separate ADT is used.

If the application distinguishes missing from null, a separate ADT or another explicitly richer model is needed, not collapsing both into a single `None`.

```flick
type alias Preferences = {
  nickname : Option String;
  quota : Number;
} deriving Schema

preferences_json : PureCodec Json Preferences
let preferences_json =
  codec (schema Preferences) {
    nickname = optional_field "nickname" String.json;
    quota =
      field "quota" Number.json_finite
      |> on_missing (fun () -> 0);
  }
```

`optional_field` and `on_missing` are accepted DSL names. A missing quota gets 0 by exactly one call of the pure factory per corresponding decode; null and an invalid value are not replaced by the default. Number.json_finite rejects a non-finite value on encode too. To exclude it in the domain, the field is declared with a refinement type.

A constructor default, a decode default and omission on encode are independent policies. `on_missing` takes a pure factory; a separate `on_missing_effect` adds its services to `rd`, and expected errors are explicitly represented as SchemaError. Encode does not omit a value because it equals the default, without a separate policy. An effectful default does not give a PureCodec/Schema certificate. [Effect constructor defaults](https://effect.website/docs/v4/schema/default-constructors).

### Effectful codec transformations

For a codec that looks a user up by ID, the type may be `Codec {Directory} none UserId User`: decode requires `Directory`, and encode takes the ID from the User already at hand. Building the codec value itself is pure; running decode is runtime. The possibility of such a lookup does not promise a round trip if the state of Directory changes.

Effect supports [effectful schema transformations](https://effect.website/docs/v4/schema/transformations). The language keeps the independent decode/encode requirements; empty rows of the general `Codec` do not turn it into a `PureCodec`.

### Full deriving Schema

`deriving Schema` is a chosen part of the language. The compiler builds a runtime witness from the typed definition, passes parameters explicitly and generates Effect Schema/combinators. It does not try to recover an erased type from an arbitrary JS value.

| Data shape | What is derived |
| - | - |
| Primitives, literals, `Unit`, `Never` | A check of the chosen domain; `Never` accepts no values |
| Records and exact tuples | Witnesses of the fields/elements, layout and a consistent policy for extra keys |
| Nominal ADTs | Checks of the discriminant and payload of each branch; agreement with the constructor ABI |
| `Option`, `Result`, collections | Standard schema constructors with explicit witnesses of the parameters |
| Generic types | A function from the schemas of the parameters used to the schema of the result |
| Higher-kinded parameters | An explicit polymorphic witness for building schemas of `f a` |
| Recursion and mutual recursion | Lazy references within a group of definitions; no infinite unfolding |
| Refinements | A pure predicate and a diagnostic; normalization is separated into the codec |
| Opaque types | A witness provided by the owner of the type; a private constructor is not exposed |
| Phantom parameters | No fictitious runtime check of something absent from the representation |

```flick
type Tree a =
  | Leaf a
  | Branch (List (Tree a))
  deriving Schema

tree_schema : Schema a -> Schema (Tree a)
let tree_schema item_schema =
  schema Tree item_schema

type alias PairOf f = {
  count : f Number;
  label : f String;
} deriving Schema

pair_of_schema :
  (forall a. Schema a -> Schema (f a)) -> Schema (PairOf f)
let pair_of_schema make_schema =
  schema PairOf make_schema
```

`List` here is the chosen immutable source API over a dense JS array. `make_schema` is applied at different types and uses higher-rank checking. `Schema a` is a witness value that is passed around, not a hidden class constraint.

Ordinary recursive data is finite; shared subtrees are allowed, and a cycle on the active decoding path gives SchemaError. Cyclic-graph codecs are not part of the first version. Recursive and mutually recursive schemas use lazy references; irregular recursion uses lazy instantiation by the actual parameter, without infinite eager unfolding. If such a contract cannot be expressed as a finite JSON Schema, the strict exporter rejects it.

Effect provides [lazy recursive and mutually recursive schemas](https://effect.website/docs/v4/schema/advanced-usage). Derivation from Flick definitions and checking of the witness parameters remain the work of the Flick compiler.

### DSL completeness and the limits of automation

The full DSL includes renaming, ADT discriminants, composition, pure/fallible/effectful transformations, refinements, defaults, whole-record checks and explicit witnesses. It elaborates into typed library constructors. An unsupported field is not silently skipped by deriving: a witness or a source diagnostic is required. A brand check of a function, a Ref or an opaque generic does not prove parametricity or the type of the contents. A snapshot of mutable/resource data remains runtime, and the captures of opaque leaves are preserved.

Completeness of deriving means there is no artificial restriction to simple records only. It does not mean that parametricity can be checked from a function type, that serialization of a live resource can be derived from a resource handle, or that the permissibility of disclosure can be chosen from `Redacted`. Such fields require an explicitly given checked witness, an opaque predicate or a separate codec. For `Ref` and resources a snapshot is a runtime operation; secrets require a declared policy.

A data schema and a wire codec remain different values. An imported Effect Schema with unknown effects gets the general `Codec`; a purity certificate is not assigned automatically. Arbitrary, JSON Schema and other external interpreters have their own conditions of applicability: for example, an arbitrary predicate does not automatically give a constructive generator. An unsupported export must require an override or give a precise diagnostic, not replace the promised type with a universal JSON schema.

### Schema exporters and compiling execution

Extensible schema integrations and strict diagnostics for an unsupported wire contract are chosen. There are two different tasks here.

**Schema export and interpretation.** The first version includes:

| Target | Fixed contract | Limitation |
| - | - | - |
| JSON Schema Draft 2020-12 | Strict export of an explicitly chosen JSON wire codec | An unsupported acceptance rule gives ExportError with a path |
| Standard Schema V1 | A sync or Promise adapter after the decoding services are satisfied | Owner/cancellation and JS input import remain explicit |
| Arbitrary | A structural generator | An unsupported refinement requires an explicit generator |
| Diagnostic formatter | Structured issues, paths and redaction | Does not reveal Redacted or an arbitrary opaque payload |
| OpenAPI 3.1 | Strict export of an `HttpApi` value: paths, methods, parameters, bodies, responses, errors and security; the schemas are the same strict JSON Schema 2020-12 | An unrepresentable endpoint schema gives ExportError with the path `group.endpoint -> field` |
| Other targets | Plugins after the first version | Separate versions and a capability matrix |

Native [JSON Schema generation](https://effect.website/docs/v4/schema/json-schema) permits a best-effort approximation. The Flick strict exporter checks exact expressibility of the chosen wire contract: an unsupported predicate, an opaque format, effectful acceptance or inexpressible recursion give ExportError with a node/path. Silent `{}` and `any` are forbidden. An explicit override is bound to a specific node. A simpler wire-shape witness can be exported separately, but it cannot be called the full validation contract of an effectful codec.

For example: `Profile.session -> JSON Schema: scoped resource has no wire codec`. The fix is a codec, an explicit data projection, or excluding the field from the chosen external representation. For statically known schemas this is a build diagnostic; for dynamically built ones it is a typed result of the export operation.

The [Effect Standard Schema adapter](https://effect.website/docs/v4/schema/standard-schema) requires satisfied decoding services and permits sync/Promise validation. The Flick adapter belongs to the owner of the provided services; a late validate after closing returns a controlled error. A call with arbitrary JS input is a runtime boundary with the chosen import contract. PureCodec permits a sync path after a safe import; the general Codec takes the Promise path. A universal runSync is not used.

**Optimizing Effect Schema execution.** v4 has [JIT and AOT schema compilers](https://effect.website/docs/v4/schema/compilers). They generate JS for the supported part of the AST and leave the other nodes to the interpreter. AOT produces a static module; the API is still experimental.

The interpreter is the mandatory baseline; AOT is an opt-in build optimization of the first version. Implicit runtime JIT is not enabled. An unsupported AOT node stays interpreted if the semantics are preserved; this does not allow approximation in wire export. The registry of compiled parsers is installed by explicit initialization of the entry point.

A fast path may repeat checks/getters on invalid input when building a detailed diagnostic; pure schema optimizations require a consistent inert data ABI. The Effect compiler skips fast paths that could repeat defaults, transformations or other effects. The Flick adapter preserves these conditions.

```text
Email.parse : String -> Result SchemaError Email
Email.value : Email -> String
```

A constant can be checked at compile time if the compiler supports the specific check. An arbitrary value requires a check at run time. Types are erased when JS is generated; the mere presence of the name `Email` does not check an external object.

## What not to promise through a refinement

Checking an immutable value can justify its nominal type. Checking that "the user exists in the DB" depends on the state of the outside world and does not guarantee existence after the result is returned. Such operations remain runtime calls, inside a transaction if necessary.

The Codec type does not prove an isomorphism. A separate library contract `lawful` fixes the successful subdomain, the chosen Eq and the permitted canonical wire form; the laws are checked by the component's tests, not by an arbitrary proof of the checker. Defaults, lookup and normalization do not get a round trip automatically.

Defaults, normalization, partial transformations and effectful lookup do not get a round-trip law automatically. A one-way migration, projection or masked output may provide only a decoder/encoder. The compiler checks types and requirements but does not prove arbitrary user-defined laws.

## Ref and type generalization

`Ref` is conveniently provided by ordinary library functions. The language must understand their runtime arrow and apply the generalization rule from section 5.

```text
Ref.make : a ->{} Ref a
Ref.get : Ref a ->{} a

Ref.set :
  forall c a.
  (Ref a)^c -> (a ->{} Unit)^c

Ref.modify :
  forall c d a b.
  (Ref a)^c -> ((a -> (b, a))^d ->{} b)^c
```

Ref is invariant in its contents. A partial application of set/modify holds the cell and its captures; a runtime write additionally checks the lifetime of the contents.

```flick
increment : Ref Number ->{} Number
let increment counter =
  Ref.modify counter (fun current ->
    let next = current + 1
    (next, next)
  )
```

`modify` performs one state-changing operation; separate `get` and `set` do not form one atomic operation. The callback of an ordinary `Ref.modify` is pure. A modification with a runtime callback requires a different synchronization contract. [Effect Ref](https://effect.website/docs/v4/state-management/ref).

Canonical Eq/Hash over the contents of a Ref are absent in the first version: a mutation could change the hash of a key. An explicit identity operation or a wrapper with a once-allocated identity key is a separate API; it does not compare the stored value. `deriving Hash` for a record with a Ref and no such wrapper/witness gives a diagnostic.

## Eq and Hash as language classes

```flick
class Eq a => Hash a =
  hash : a -> Number
```

The law: if `eq x y`, the hashes coincide. The converse is false: collisions are allowed. Derived `Eq` and `Hash` must use the same semantics of fields. Typing a user-defined instance does not prove this law. [Effect Hash](https://effect.website/docs/v4/trait/hash).

Canonical Eq/Hash map to consistent native Effect protocols. For alternatives, an `EqHash a` witness with both methods is provided, along with the constructors `HashSet.with witness` / `HashMap.with witness`. The collection stores the witness, and the adapter packs the keys into native protocol wrappers and unwraps them on output. Insertion/lookup always use the same witness; it cannot be changed on a populated collection. A union of collections with different witnesses requires an explicit rehash into the chosen witness. This keeps the Effect collection engine at the price of wrappers. [Effect structural Equal](https://effect.website/docs/v4/trait/equal).

## Option and representation compatibility

The standard `Option` has the source constructors `None` and `Some`, and at runtime the real `Option.none()` and `Option.some(value)` from Effect. Pattern matching is checked by the source frontend and reads their native representation. [Effect Option](https://effect.website/docs/v4/data-types/option).

This binding is determined by the identity of the standard type. A user-defined `type Option ...` or an object with `_tag = "Some"` does not get the Effect ABI automatically. When importing ordinary nullable JS values, the conversion is performed by an explicit binding.

A pure function over `Option` is generated as an ordinary JS function. Importing an Effect data module does not mean starting a fiber. The backend uses the standard Effect implementations and may eliminate unused imports.

## The main collection and the List representation

The name **`List a`** and the representation **dense JS array with an immutable source API** are chosen. All `[a, b]` and `[]` create a List; there is no hidden overloading of the literal by expected type. A separate `Chunk a` uses the native Effect Chunk. Mutable foreign arrays are represented by `Js.Array a` through a runtime API; there is no second equivalent Array name in the prelude.

| Representation | Strengths | Cost |
| - | - | - |
| A JS array with an immutable API: the chosen List | A direct JS ABI, indexed access and ordinary loops; simple code generation | Prepending, immutable update and concatenation usually copy O(n) elements |
| Cons-list `Nil / Cons` | O(1) prepend, head and tail; tail sharing; familiar ML pattern matching | Indexed access is O(n); more objects and GC load; translation to JS arrays is needed |
| Effect `Chunk` | A ready-made immutable collection for repeated concatenation | An extra structure and operation cost; for many tasks a plain array is simpler |

List has O(1) length/index and an ordinary loop traversal; prepend, immutable update, tail and concat usually copy O(n) elements. O(1) cons/tail are not promised. A recursive traversal with repeated `slice(1)` can become quadratic; the standard traversals use indexed loops. A future cons-list will be a separate type with a different representation, not a change to the List ABI.

The main literal always means List. Chunk, mutable JS arrays and other containers are created by explicit constructors/conversions. Missing elements of a sparse JS array do not pass as an ordinary List without an adapter.

TS `ReadonlyArray` does not guarantee actual immutability. FFI must copy, transfer ownership, or work with an opaque mutable handle. A shallow copy suffices only for the sequence itself: nested records also require a consistent contract. Unsafe inputs must not reach the trusted pure API.

The [Effect Chunk documentation](https://effect.website/docs/v4/data-types/chunk) recommends Chunk primarily for frequent concatenation and separately warns of extra costs in other scenarios. All these collections remain a library over the same Effect backend.

## Redacted and external serialization

`Redacted a` is useful as an opaque type with masked output and explicit extraction of the value. It does not encrypt data. Hiding in logs does not mean a ban on serialization.

In [Effect Schema v4](https://effect.website/docs/v4/schema/effect-data-types), the canonical JSON encoding of Redacted may reveal the inner value; the setting `disallowJsonEncode: true` is available. Accepted for the language: deriving does not create a revealing external codec without opt-in. The ban, intentional transfer of a secret and a masked projection are set explicitly; a masked projection is a one-way encoder and does not promise a round trip.

```text
Redacted.make : a ->{} Redacted a
Redacted.reveal : Redacted a ->{} a
```

The canonical Effect `Redacted` is used, whose contents external code can erase through `wipeUnsafe`. Therefore creation and reading in the source API are conservatively runtime: creation allocates a standalone erasable object, and reading depends on its current state. Merely freezing the JS wrapper does not prove immutability of the hidden contents. Erasure through FFI is also runtime; reading an erased value gives a defect unless the adapter explicitly translates it into a declared error. Automatic pure Eq/Hash over the contents are not provided. [Effect Redacted](https://effect.website/docs/v4/data-types/redacted).

## The JavaScript numeric model

The decision is made: ordinary numbers follow JavaScript. The source uses `Number` with the direct representation of the JS `number`; for arbitrary integers there is a separate `BigInt`. `BigDecimal`, `Duration` and other special types remain library types with their own operations.

| Aspect | Contract |
| - | - |
| `Number` | IEEE 754 binary64, including NaN, both infinities and signed zero |
| Literals | An ordinary numeric literal has type `Number`; the suffix `n` explicitly selects `BigInt` |
| Arithmetic | JS rounding, overflow, division and remainder; no promise of exact integer arithmetic for all of Number |
| Safe integers | `SafeInteger` is a refinement, not a different representation; ordinary addition does not preserve it automatically |
| Bitwise operations | Follow the JS width conversions and must have explicitly described operations |
| `BigInt` | A separate numeric domain; no implicit mixing with Number |
| Cross-type coercions | The JS number representation does not require implicit addition of a string and a number or loose equality |
| JSON | The Number domain is wider than JSON; a finite codec rejects an unsupported value, a lossless codec uses an explicit tagged/string form |

The model corresponds to [ECMAScript Number and BigInt](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html). Domain errors of BigInt primitives, for example division by zero, keep JS exceptions; when executed inside Effect these are defects. For a recoverable variant the library provides a checked operation with Result or a declared typed error. Neither ordinary `Number` division by zero nor NaN should be silently turned into a typed error.

`Finite`, `SafeInteger`, `NonNegative` are possible refinements for data boundaries. Their arithmetic returns `Number` until an operation separately guarantees preservation of the constraint. `Schema Number` reflects the whole chosen domain; the JSON restriction is set by the codec. Native [JSON.stringify converts non-finite numbers to null](https://tc39.es/ecma262/multipage/structured-data.html), so it cannot be used as a "lossless Number codec".

### Eq, Hash and Ord for Number

| Equality | NaN equals NaN | +0 equals -0 | Cost |
| - | - | - | - |
| JS strict equality | No | Yes | Non-reflexivity complicates the Eq laws and the behavior of keys |
| `Object.is` | Yes | No | A consistent hash and an adapter relative to canonical Effect equality are needed |
| Canonical Effect equality: chosen | Yes | Yes | A separate function is needed for exact JS `===` |

The canonical `Eq Number` and the operator `==` use SameValueZero: NaN equals NaN, +0 equals -0. This matches the checked [Effect Equal](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/Equal.ts#L187-L205); the [native Hash.number](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/Hash.ts#L311-L350) is consistent with it. `Number.strict_equal` provides JS `===`, `Number.same_value` provides Object.is. Hash is not a stable external identifier across runtime versions.

`Ord Number.compare` defines a total order: −Infinity < finite numbers < +Infinity < NaN; signed zeros compare as EQ, all NaNs as EQ. `compare x y = EQ` is equivalent to canonical Eq. Generic sorting and `Ord.less` use this order. The primitive Number/BigInt `<`, `<=`, `>`, `>=` keep JS semantics: for example, any such comparison of a Number with NaN is false. This is an explicit distinction between numeric operators and a lawful Ord, not a hidden change of JS arithmetic.

Even if Eq considers +0 and -0 equal, the compiler is not entitled to replace one with the other everywhere: arithmetic can distinguish them. Collection equality is not permission for any substitution transformation.

## BigDecimal and Duration

BigDecimal is a separate native Effect type with an integer coefficient and a scale. The first version does not introduce `dec` literals and does not overload Number arithmetic. Values are created from a string or a pair of BigInt/checked scale; the path through Number explicitly acknowledges the binary64 rounding that has already happened. [Effect BigDecimal](https://effect.website/docs/v4/data-types/bigdecimal).

The source decimal API has no hidden global precision. Exact addition/subtraction/multiplication preserve a finite decimal result; an unrepresentable scale gives a checked DecimalError. `divide_to_scale` necessarily takes a finite integer safe-integer scale and a rounding policy: TowardZero, Floor, Ceiling, HalfEven, HalfAwayFromZero or Exact. Division by zero is a DecimalError; Exact rejects a result that requires rounding. The result is `Result DecimalError BigDecimal`, and the native representation is preserved. The adapter computes the quotient/remainder without an intermediate Number; native division with its own defaults is not substituted for this contract. Unsafe native operations remain explicitly named bindings.

Duration is a native non-negative duration, including the explicitly chosen `Duration.infinity`; it is not a timestamp or a calendar period. The first version uses ordinary checked constructors nanos(BigInt), millis(Number), seconds(Number), without `ms`/`s` literals. A negative value, NaN, an implicit infinity and overflow in unit conversion give `Result DurationError Duration`. The exact nanosecond form starts from BigInt; Number-based forms keep the chosen JS rounding and do not promise greater precision. A lossy conversion to Number is named explicitly; infinity is not silently turned into a finite number. [Effect Duration](https://effect.website/docs/v4/data-types/duration).

For BigDecimal/Duration, consistent native Schema, Eq, Hash, Ord and Show are available through checked adapters. Special literals and numeric operator overloading are deferred beyond the first version. Checked library constructors leave a single numeric syntax for Number/BigInt and do not require a new runtime.

The first target release includes full deriving Schema, explicit codecs, nominal refinements with a runtime check, native Effect types, a library Ref, the Eq/Hash bridge and runtime access to Redacted. A compiler spike may implement these mechanisms in stages, but completeness of deriving remains a language requirement.
