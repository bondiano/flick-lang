---
title: "Types and classes"
source_hash: 4c088b3d69f04647
---

## Computations and kinds

The representation of a computation is `Eff requirements errors result`. The result comes last so that `Eff r e` is a constructor of kind `Type -> Type`.

```text
Unit, Never, Float, BigInt : Type
Option, List, Ref, Redacted, Schema : Type -> Type
Result, PureCodec : Type -> Type -> Type

Eff : ServiceRow -> ErrorRow -> Type -> Type
Layer : ServiceRow -> ErrorRow -> ServiceRow -> Type
Codec : ServiceRow -> ServiceRow -> Type -> Type -> Type
Errors : ErrorRow -> Type

FieldRow = Row Type
Region, CaptureRow, Symbol : Kind

Eff r e : Type -> Type
```

`FieldRow = Row Type` here denotes an equality of kind shapes, not a user type declaration. `ServiceRow`, `ErrorRow` and `CaptureRow` are distinct built-in kinds. Field labels have kind `Symbol`; service/error identities are nominal and are not replaced by string labels. The full table of built-in types and kinds is in [Kinds and types](../../reference/types/).

On the JS target, `Eff r e a` maps to `Effect<A, ErrorUnion<E>, ServiceUnion<R>>`; the source rows have their own nominal identifiers, normalization and constraint rules, and an empty row gives `never`. The Go target runs the same descriptions on `runtime-go/flickrt` ([codegen](../../reference/codegen/#go-target)).

## Notation and joining of rows

In the position of a single row, the following are used: `none`, a variable `r`, a closed row `{Users, Log}` or an open row `{Users, Log | r}`. In an arrow, the outer row braces are omitted: `a ->{Users, Log; NotFound} b` and `a ->{Users | r; NotFound | e} b`. Here `| r` and `| e` denote the tail of a row. Without a tail the list is closed. The order of elements does not matter, and repetitions of one nominal element are normalized.

`r1 + r2` and `e1 + e2` denote the union of rows of one kind, for example `Eff (r1 + r2) (e1 + e2) a`. This is a built-in operation of the row solver, not a user type family. A signature gives an upper bound: an implementation may require fewer services or allow fewer errors. Extending a row does not turn a runtime arrow into a pure one.

`Errors e` turns an error row into an ordinary type of values: `Errors none = Never`, `Errors {NotFound} = NotFound`. With two or more elements, or with a row variable, `Errors e` is an opaque type: its value comes only from the `x`/`_` branch of `try` and goes to `from_result`, `Failure.rethrow` or generic code; to take it apart by error, write a `try` with error branches over `from_result (Err e)`. This is a fixed projection of the checker. An arbitrary type cannot be automatically turned back into a row: a generic bridge needs a saved row `e` or an annotation, and for a single declared error a singleton row is available.

## An HKT example

```flick
class Functor (f : Type -> Type) =
  map :
    forall c d a b.
    (forall v. a^v -> b)^c -> ((f a)^d -> (f b)^(c + d))^c

instance Functor Option =
  map :
    forall c d a b.
    (forall v. a^v -> b)^c ->
    ((Option a)^d -> (Option b)^(c + d))^c
  let map f value =
    match value with
    | None -> None
    | Some x -> Some (f x)
```

This is the `Functor` of the Prelude. Its `map` takes a pure callback. `c` bounds its captures, `d` the captures of the input container; the result conservatively keeps `c + d`, because an abstract `f` may defer the application and retain the input/callback. `v` bounds each callback argument independently: a transformer can receive its state or environment later, when run. Partial application stores only the callback. `Option` applies it immediately; `Eff r e` creates a cold description.

## Choosing instances and dictionaries

The checker checks kinds, infers types and constraints, resolves instances and inserts explicit dictionary arguments. At runtime a dictionary is a value with the method implementations; HKT does not need a separate type executor.

```flick
class Show a =
  show : a -> String

render : Show a => a -> String
let render value =
  show value
```

```ts
interface Show<A> {
  readonly show: (value: A) => string
}

const render =
  <A>(dictionary: Show<A>) =>
  (value: A): string =>
    dictionary.show(value)
```

The details of resolution, the instance rules and their diagnostics are in [Classes and instances](../../reference/classes/).

## The type core

- Higher-rank callbacks with a nested `forall`, checking of full signatures and type inference inside the body.
- Nominal ADTs, exact structural records with row polymorphism and exhaustive `match`.
- GADTs: a constructor names the type it builds (`| IntLit Int : Expr Int`) and may hide the types of its fields; a `match` branch learns the type of the matched value under a complete signature, and exhaustiveness asks no branch for an impossible constructor. Indices are types, finite kinds and `Symbol`; regions, rows and captures are not refined.
- HKT with type constructors and partial application.
- Multiparameter classes, superclasses and functional dependencies; the row solver has fixed deterministic primitives.
- Unique canonical instances without overlap, orphans and local implicit lookup; ownership is determined by the package.
- Explicit interfaces of public modules.
- Polymorphic recursion only against an explicitly written full `forall` signature.
- Generalization of provably pure constructions only; runtime results are monomorphic.

SQL/UI use built-in row primitives, ordinary HKT, restricted class relations and typed ASTs.

### Not in Flick

- Associated types: a relation between types is a functional dependency or an explicit witness.
- Type families, open or closed: rows have built-in primitives instead.
- Overlapping instances, instance chains and priorities.
- Orphan instances: a nominal wrapper or an explicit dictionary takes their place.
- Local implicit instances: an alternative behaviour is an explicit value (a comparator, an `EqHash` witness).
- Width subtyping of records.
- Impredicative instantiation and general higher-rank inference.
- Generalization by variance (a relaxed value restriction).
- Indexed or graded monads.

## Polymorphic callbacks

A nested `forall` in a full signature means that the called function may apply one callback at different types. Ordinary expressions still get local inference; at the boundary with a polymorphic argument the checker uses the expected signature. HKT and the rank of polymorphism remain different properties.

| Rule | Consequence |
| - | - |
| Explicit nested `forall` | The polymorphism of the argument is visible in the full signature; there is no need to guess it from several calls |
| Bidirectional checking | The checker infers ordinary types and checks an expression against a given polymorphic type |
| Predicative instantiation | An ordinary type variable is instantiated with a monomorphic type; an arbitrary `List (forall a. ...)` is not allowed |
| Constraints inside `forall` | On every application of the callback, dictionaries for the corresponding type are passed |
| Rows in the outer scope | A callback may be polymorphic in its result while keeping the same set of services and errors |
| Capture checking and skolem escape | A full signature does not allow hiding a mutable alias or passing off a fixed type as an arbitrary one (`FLICK-T007`) |

A simple example: one callback is called at `Float` and at `String` in the same body.

```flick
apply_twice :
  (forall a. a -> a) -> (Float, String)
let apply_twice transform =
  (transform 42, transform "hello")

render_twice :
  (forall a. Show a => a -> String) -> (String, String)
let render_twice render =
  (render 42, render "hello")
```

A runtime callback can be polymorphic too:

```flick
# type alias User = { name : String }
#   deriving Schema
# type alias Settings = { theme : String }
#   deriving Schema
#
decode_pair :
  (forall a. Schema a -> Unknown ->{r; SchemaError} a) ->
  Unknown -> Unknown ->{r; SchemaError} (User, Settings)
let decode_pair decode user_input settings_input =
  let user = decode User.schema user_input
  let settings = decode Settings.schema settings_input
  (user, settings)
```

Here `a` changes on every call, while `r` belongs to the outer signature. For the example, `User`, `Settings` and their Schema are assumed to be declared.

## Record structurality

Records are **exact closed structural records with field-row polymorphism**. `type alias User = { ... }` is a transparent alias of a shape; nominality comes from a nominal declaration (an ADT, an `unboxed type`, a `refined type`). ADTs are nominal. There is no implicit width subtyping: extra fields are accepted only through an explicitly open row tail or are removed by an explicit projection. Width subtyping would keep hidden fields at runtime and complicate equality and exact schemas.

A field row has kind `FieldRow`, equivalent to `Row Type`; it is not a row of services or errors. Labels are unique; their order is not part of the type. The module `Record` provides the built-in `HasField`, `Lacks`, `Project`, `Remove`, `UnionDisjoint`, `Map` over a constructor of kind `Type -> Type`, and `Labels`, the canonical enumeration of labels ([row primitives](../../reference/classes/#row-primitives)). User instances for these primitives are forbidden (`FLICK-C012`). `UnionDisjoint` rejects a repeated label; the union of service/error rows, by contrast, normalizes a repetition of one nominal element.

```flick
name :
  forall fields. { name : String | fields } -> String
let name value =
  value.name

project_name :
  forall fields. { name : String | fields } -> { name : String }
let project_name value =
  { name = value.name }
```

`name` allows extra fields through `fields`. `project_name` creates an object with only the field `name`; this is not a cast that hides keys. The update `{ record with field = value }` replaces an existing field with a value of the same type. `Record.extend` requires `Lacks`; `Record.merge_disjoint` requires disjoint rows. Changing a field's type is done by explicit remove/extend or by a new record. There is no hidden overwrite on spread. The values of the fields are evaluated in source order.

### Records, instances and FFI

Two transparent aliases of one shape are one type and do not get different global `Eq` or `Hash`. For a closed record, the canonical structural instance applies when each field has instances. An open row requires constraints on the whole tail. Compiler-owned structural instances cannot be overridden for an individual alias.

For different domain semantics, a nominal wrapper is introduced. An alternative comparison without a new type is an explicit `EqHash` witness, which a `HashSet` or `HashMap` keeps as part of its type ([hash collections](../../reference/stdlib/#hash-collections)).

A record is a plain JS object with exactly its fields (on the Go target, a `*flickrt.Record`). Flick data crosses a foreign binding as a copy, so JS cannot change a Flick value it was given ([FFI](../../reference/ffi/#js-values)). Untrusted input becomes a record only by decoding through a schema or codec, which refuses keys it does not name unless the codec goes through `ignore_extra_fields` ([schemas](../../reference/schema/)).

## Classes and dependencies between types

A class may have several parameters of any kind and superclasses. `Show a` selects an implementation for one type; `Functor f` has a single parameter of kind `Type -> Type`. A relation between independent types needs several parameters:

```flick
# type ConversionError = ConversionError String
#
class Convert input output =
  convert : input -> Result ConversionError output
```

Both `Convert String Email` and `Convert String UserId` are useful here, so this class has no functional dependency. A serialization format is an explicit codec value (`Codec rd re i a`), not a global instance.

### Functional dependencies

```flick
class Collection c a | c -> a =
  empty : c
  insert : forall captures. a^captures -> (c -> c)^captures
```

The qualifier on insert keeps the value a in the partially applied function; the concrete collection type also keeps the captures of its elements.

The dependency `c -> a` reads: “a concrete collection type determines the unique element type”. For example, the heads `Collection UserSet User` and `Collection NameSet String` are possible.

```flick
# class Collection c a | c -> a =
#   empty : c
#   insert : forall captures. a^captures -> (c -> c)^captures
# type alias User = { name : String }
# type UserSet = UserSet (List User)
# instance Collection UserSet User =
#   let empty = UserSet []
#   let insert user set = set
#
empty_users : UserSet
let empty_users =
  empty
```

The expected `UserSet` also determines `User`. Without the dependency the method would have the type `Collection c a => c`, where `a` is not fixed by the result; a separate basis for choosing the dictionary unambiguously would be required.

The pair `Collection UserSet User` and `Collection UserSet String` violates the FD (`FLICK-C004`). The reverse direction is not promised: one `User` can be stored in different collections. After expanding aliases and normalizing built-in row expressions, coverage starts with the variables of the determining head arguments and closes over dependencies in the context and its instantiated superclasses. A dependency contributes its outputs only when all its inputs are covered; every determined head variable must belong to this least fixed point (`FLICK-C005`). Each head dependency is checked independently, so unrelated constraints and unseeded cycles do not help.

Context-assisted coverage permits capability lifting, not context-based selection. Instances are selected by their complete heads before their contexts are solved. Incompatible FD outputs are rejected, as are overlapping heads. For otherwise non-overlapping heads whose determining positions unify, the determined positions must already be equal under that determining substitution alone; contexts cannot justify additional output bindings. Context variables must still occur in the head, each constraint must be structurally smaller, and variable occurrences may not increase. These restrictions do not prove unbounded termination of FD improvement: every resolution and improvement retry consumes the configured step/depth budget, and exhaustion is `FLICK-C009`, never success.

## Coherence and ownership

Coherence means that one constraint gets one implementation. For collections, changing `Eq`, `Hash` or `Ord` between insertion and lookup would break their behavior.

Instance heads, after expanding aliases, must not unify (`FLICK-C003`); their context is not used to resolve overlap. All context variables must be present in the head. Recursive class constraints must shrink structurally without increasing the number of occurrences of a variable (`FLICK-C007`); the superclass graph is acyclic (`FLICK-C008`). This is a conservative termination condition, with no analogue of `UndecidableInstances`. Exceeding the solver budget produces a diagnostic and never means successful resolution. `Eq a => Eq (List a)` is allowed.

### An external type

Let `Hash` be defined by the standard library and `ExternalId` by an SDK. An adapter package that owns neither the class nor the original type cannot declare `Hash ExternalId`: orphans are forbidden.

```flick
# import Sdk exposing (ExternalId)
#
unboxed type LocalExternalId =
  | LocalExternalId ExternalId
```

Now the adapter owns a nominal type and can give it canonical instances. `unboxed` leaves the `ExternalId` itself at run time: the wrapper allocates nothing and goes into the SDK as is, while identity and instances stay with the nominal type. The transparent alias `type alias LocalExternalId = ExternalId` creates no new identity. Other paths are an instance at the owner of the class/type, or an explicitly passed dictionary.

Ownership is determined by the **package** (`FLICK-C006`): an instance can be declared by the package of the class or by the package of the external nominal constructor of the **first parameter of the head**. A transparent alias and a structural record do not create an owner. For MPTC, ownership of only a later parameter is not enough: the package of `LocalEmail` does not get the right to declare a foreign `Convert String LocalEmail`. For that, an explicit codec/dictionary, a wrapper of the first parameter or an instance at the owner of the class is needed. A binding does not change the identity of a foreign type.

A package is a project or one of the libraries it compiles from their sources (a path, git or url dependency, or an npm package with `source-dir`); the core modules and `std` are one package, the built-in one. An instance is visible wherever its class and its head type are: a library's instances come with it into every dependent, and two libraries that each own one side cannot both declare the instance, since only the owner of the class or of the first type may. So every program sees at most one instance for each head, whichever libraries it combines.

The set of instances a module sees is determined by its dependency graph, not by the order or form of `import`; conflicts are diagnosed when the whole accessible graph is checked.

## Generalization in the presence of Ref

Generalization means that the type of a binding can be instantiated anew at each use. For `identity : forall a. a -> a`, two calls can independently choose `Float` and `String`. A single mutable cell must keep one content type. An explicit `forall` signature cannot make unsafe sharing of state acceptable.

```flick fail
invalid : Unit ->{} String
let invalid () =
  let cell = Ref.make None
  Ref.set cell (Some 42)
  match Ref.get cell with
  | Some value -> String.trim value
  | None -> ""
```

The program is rejected: the write `Some 42` fixes the cell's content as `Option Int`, while `String.trim` requires `String`. If `cell` were given `forall a. Ref (Option a)`, the write and the read could choose different types for one object.

**Provably pure constructions of values** are generalized, only over variables that are not free in the environment and not tied to local skolems, region/capture obligations or ambiguous constraints. A binding without a signature gets no class constraints: the variables of its unsolved constraints and of its number literals stay monomorphic, and constrained polymorphism needs a signature. The result of a runtime binding is not generalized even with `->{}` (`FLICK-T013`), including in a covariant immutable type such as `Option a`. A full signature does not cancel these checks. Mutable containers are invariant. A trusted pure FFI must observe the same purity contract.

```flick
make_empty : forall a. Unit ->{} Ref (Option a)
let make_empty () =
  Ref.make None

use_separate_cells : Unit ->{} Unit
let use_separate_cells () =
  let numbers = make_empty ()
  let names = make_empty ()
  Ref.set numbers (Some 42)
  Ref.set names (Some "Ada")
```

The value of the function `make_empty` is polymorphic: creating the closure is pure. Each runtime call allocates a new cell; `numbers` itself is monomorphic and stores only `Option Int`, and `names` only `Option String`. This distinction preserves the useful polymorphism of factories without allowing the type of already created state to change.

### Pure calls and cold descriptions

```flick
make_identity : Unit -> (a -> a)
let make_identity () =
  fun value -> value

identity_again : forall a. a -> a
let identity_again =
  make_identity ()
```

The right-hand side `make_identity ()` is a call, not a syntactic value, but it is pure: no state is allocated, and the variable `a` is not captured from the environment. So `identity_again` is generalized.

```flick
fresh_cell : forall a. Eff none none (Ref (Option a))
let fresh_cell =
  task { Ref.make None }

use_tasks : Unit ->{} Unit
let use_tasks () =
  let numbers = await fresh_cell
  let names = await fresh_cell
  Ref.set numbers (Some 42)
  Ref.set names (Some "Ada")
```

`task` creates a cold description. Therefore `fresh_cell` can be generalized: each `await` runs `Ref.make` anew. The resulting cells are again monomorphic. `await` does not cache the result and does not start a separate fiber.

```flick
capture_cell : Ref (Option a) -> Eff none none (Ref (Option a))
let capture_cell cell =
  task { cell }
```

Here repeated runs return the one captured cell. The type `a` is tied to the argument `cell`: a local description cannot generalize a variable that is free in the environment. The coldness of `Eff` by itself does not prove the absence of captures.

## Deriving and class laws

`deriving Eq, Ord, Hash, Show` creates class instances (`Hash` and `Ord` only together with `Eq`); `deriving Generic` gives a type its representation as a sum of products, and `deriving C` of a class of one parameter with a default for each method an instance that takes them, so a class written once over `Generic` derives for every type; `deriving Schema` creates a runtime witness of the data. Deriving Schema covers records, ADTs, tuples, generics, recursive types and refinements; it does not declare a type class of the same name. The contract is in [Schema and standard data types](../07-schema/).

Checking an instance confirms its type but does not prove the class laws. Derived `Eq`, `Ord` and `Hash` use the same fields. User-written implementations require separate law checks; serialization and storage format are specified by an explicit codec, not by a single global instance.

## Monad and joining rows

The Prelude declares `Functor`, `Applicative`, `Monad`, `Alternative`, `Foldable` and `Traversable` over `f : Type -> Type`; `Functor`, `Applicative` and `Monad` have instances for `Eff r e`. `Monad`'s method is `flat_map : forall c d a b. (forall v. a^v -> (m b)^(c + v))^c -> ((m a)^d -> (m b)^(c + d))^c`; capture rows give upper bounds, and the result keeps the captures of the original description and of the continuation.

`Monad (Eff r e)` fixes `r` and `e`; their explicit or inferred widening to a common row is allowed. Direct runtime calls use a separate built-in sequencing that joins rows (`Eff (r1 + r2) (e1 + e2) b`), without going through the `Monad` instance; the checker computes the unions of requirements/errors for direct calls and preserves the captures of descriptions and callbacks. A generic `do` elaborates to the `Monad` and `Alternative` methods of the Prelude directly, whatever names are in scope; with `Monad (Eff r e)` it keeps the fixed rows, which is why direct calls stay the main style for Eff.
