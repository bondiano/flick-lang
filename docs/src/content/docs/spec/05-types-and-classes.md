---
title: "5. The HKT type core and type classes"
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

`FieldRow = Row Type` here denotes an equality of kind shapes, not a user type declaration. `ServiceRow`, `ErrorRow` and `CaptureRow` are distinct built-in kinds. Field labels have kind `Symbol`; service/error identities are nominal and are not replaced by string labels.

In code generation, `Eff r e a` maps to `Effect<A, ErrorUnion<E>, ServiceUnion<R>>`. The source rows have their own nominal identifiers, normalization and constraint rules. TS unions serve as the output representation; an empty row gives `never` in both cases.

## Notation and joining of rows

In the position of a single row, the following are used: `none`, a variable `r`, a closed row `{Users, Log}` or an open row `{Users, Log | r}`. In an arrow, the outer row braces are omitted: `a ->{Users, Log; NotFound} b` and `a ->{Users | r; NotFound | e} b`. Here `| r` and `| e` denote the tail of a row. Without a tail the list is closed. The order of elements does not matter, and repetitions of one nominal element are normalized.

`r1 + r2` and `e1 + e2` denote the union of rows of one kind, for example `Eff (r1 + r2) (e1 + e2) a`. This is a built-in operation of the row solver, not a user type family. A signature gives an upper bound: an implementation may require fewer services or allow fewer errors. Extending a row does not turn a runtime arrow into a pure one.

`Errors e` turns an error row into an ordinary type of values: `Errors none = Never`, `Errors {NotFound} = NotFound`, and for several elements the result is a sum of the nominal errors. This is a fixed projection of the checker. An arbitrary type cannot be automatically turned back into a row: a generic bridge needs a saved row `e` or an annotation, and for a single declared error a singleton row is available.

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

The ordinary `Functor.map` takes a pure callback. `c` bounds its captures, `d` the captures of the input container; the result conservatively keeps `c + d`, because an abstract `f` may defer the application and retain the input/callback. `v` bounds each callback argument independently: a transformer can receive its state or environment later, when run. Partial application stores only the callback. `Option` applies it immediately; `Eff r e` creates a cold description.

## Choosing instances and dictionaries

The frontend checks kinds, infers types and constraints, resolves instances and inserts explicit dictionary arguments. At runtime a dictionary is a value with the method implementations; HKT does not need a separate type executor.

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

A similar passing of implementations is used in PureScript. [Type classes and generated dictionaries](https://book.purescript.org/chapter6.html).

## Requirements for the type core

- Higher-rank callbacks with a nested `forall`, checking of full signatures and type inference inside the body.

- Nominal ADTs, exact structural records with row polymorphism and exhaustive `match`.

- HKT with type constructors and partial application.

- Multiparameter classes, superclasses and functional dependencies are part of the first version; the row solver has fixed deterministic primitives.

- Unique canonical instances without overlap, orphans and local implicit lookup; ownership is determined by the package.

- Explicit interfaces of public modules.

Associated types, user-defined open and closed type families, instance chains, impredicative instantiation and general higher-rank inference are not part of the first version. Polymorphic recursion is checked only against an explicitly written full `forall` signature. SQL/UI use built-in row primitives, ordinary HKT, restricted class relations and typed ASTs.

Generalization of an ordinary pure `let` and of the result of an effectful binding cannot be mechanically considered the same. State and FFI need generalization rules and a value restriction that prevent unsafe sharing of polymorphic mutable values.

## Polymorphic callbacks

Higher-rank callbacks are part of the language from the start. A nested `forall` in a full signature means that the called function may apply one callback at different types. Ordinary expressions still get local inference; at the boundary with a polymorphic argument the checker uses the expected signature. HKT and the rank of polymorphism remain different properties.

| Rule | Consequence |
| - | - |
| Explicit nested `forall` | The polymorphism of the argument is visible in the full signature; there is no need to guess it from several calls |
| Bidirectional checking | The checker infers ordinary types and checks an expression against a given polymorphic type |
| Predicative instantiation | An ordinary type variable is instantiated with a monomorphic type; an arbitrary `List (forall a. ...)` is not allowed automatically |
| Constraints inside `forall` | On every application of the callback, dictionaries for the corresponding type are passed |
| Rows in the outer scope | A callback may be polymorphic in its result while keeping the same set of services and errors |
| Capture checking and skolem escape | A full signature does not allow hiding a mutable alias or passing off a fixed type as an arbitrary one |
| Scope of the first version | Nested `forall` in annotated function types; general higher-rank inference and impredicativity are not promised |

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
decode_pair :
  (forall a. Schema a -> Unknown ->{r; SchemaError} a) ->
  Unknown -> Unknown ->{r; SchemaError} (User, Settings)
let decode_pair decode user_input settings_input =
  let user = decode (schema User) user_input
  let settings = decode (schema Settings) settings_input
  (user, settings)
```

Here `a` changes on every call, while `r` belongs to the outer signature. For the example, `User`, `Settings` and their Schema are assumed to be declared.

The cost of higher-rank is bidirectional checking, checking of rigid type variables, instantiation and escape diagnostics. Polymorphic recursion and mutable generalization remain separate checks. This approach to annotated boundaries is described in [GHC](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/rank_polymorphism.html#type-inference); the presence of general signatures above functions makes it natural for Flick.

## Record structurality

The choice is **exact closed structural records and field-row polymorphism**. `type alias User = { ... }` is a transparent alias of a shape; nominality is introduced by a separate opaque/newtype declaration. ADTs are nominal. There is no implicit width subtyping: extra fields are accepted only through an explicitly open row tail or are removed by an explicit projection.

| Model | What is convenient | Cost |
| - | - | - |
| Exact structural records + open field rows — chosen | Literals without constructors; functions accept any explicitly described extra row; the exact shape suits Schema | Needs unification of field rows, uniqueness of labels and constraint rules for the open tail |
| Structurality with width subtyping | A large object passes into an API of a smaller shape almost as in TS | Hidden fields remain at runtime; inference, higher-rank checking, equality and agreement with exact schemas get harder |
| Nominal records | Different domains can have identical fields and their own instances | Extra constructors/wrappers and conversions at the FFI; less convenience for everyday data |

A field row has kind `FieldRow`, equivalent to `Row Type`; it is not a row of services or errors. Labels are unique; their order is not part of the type. The first version provides the built-in `HasField`, `Lacks`, `Project`, `Remove`, `UnionDisjoint`, `Map` over a constructor of kind `Type -> Type`, and canonical enumeration of labels. User instances for these primitives are forbidden. `UnionDisjoint` rejects a repeated label; the union of service/error rows, by contrast, normalizes a repetition of one nominal element.

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

For different domain semantics, a nominal wrapper is introduced. At the boundary with native Effect Equal/Hash, such a wrapper has a real runtime shell with its own protocol. Erasing it is permitted only inside a region where the compiler has proved that identity/protocol is not observed through the FFI or collections. An alternative comparison without a new type is passed as an explicit, consistent `EqHash` witness in a dictionary-aware collection.

On input from JS, an exact record by default rejects extra keys; an explicitly chosen projection policy creates a new object of the required shape. An ordinary cast does not do this. The representation of a record is a plain JS object; an untrusted boundary by default copies supported immutable data and does not hand out a mutable alias of the original object. A direct readonly façade without copying is allowed only under a trusted ownership contract. `readonly` from a `.d.ts` does not prove such a contract by itself.

[PureScript row polymorphism](https://github.com/purescript/documentation/blob/master/language/Types.md) shows an approach with field rows. In [Effect Equal](https://effect.website/docs/v4/trait/equal), plain objects are compared structurally, so extra fields that are silently retained affect the behavior of collections.

## The chosen class core and the cost of alternatives

The list of decisions consists of independent axes: the number of class parameters; the way relations between types are expressed; the selection and ownership of instances; the generalization of let-bindings. HKT and higher-rank callbacks are already chosen and do not automatically require enabling all the other mechanisms.

### What a single parameter gives

```flick
class Show a =
  show : a -> String

unboxed type PrintableUser =
  | PrintableUser User

instance Show PrintableUser =
  show : PrintableUser -> String
  let show value =
    match value with
    | PrintableUser user -> user.name

render : Show a => a -> String
let render value =
  show value
```

PrintableUser is a nominal wrapper owned by the instance's package. The transparent record alias User by itself does not give that right for a foreign class.

`Show a` selects an implementation for one type. The parameter may also have kind `Type -> Type`: `Functor f` has a single one, `f`. So the single-parameter core supports Functor, Foldable, Traversable, Monad and `Functor (Eff r e)`. A higher-rank callback with `forall a. Show a => ...` remains available.

The cost of the restriction is visible in a relation between two independent types:

```flick
class Convert input output =
  convert : input -> Result ConversionError output
```

Both `Convert String Email` and `Convert String UserId` are useful here. In the single-parameter core this is expressed by separate functions or explicit converter/codec values. Our `Codec rd re i a` already allows passing the needed conversion directly; it does not need a single global instance.

### The initial set

The first version includes **multiparameter classes, superclasses and FD**, unique canonical instances without overlap, package ownership and generalization of provably pure constructions. Runtime results are monomorphic. This set is needed for the relations between parameters of the SQL/UI eDSL; the built-in operations on rows remove some of the tasks that PureScript libraries solve by complex instance search.

Associated types, general type families and a relaxed value restriction by variance are deferred beyond the first version. The comparisons below explain the cost of these mechanisms; they are not decisions left open.

## Class parameters and dependencies between types

| Mechanism | Example use | Cost |
| - | - | - |
| One parameter and superclasses | `Eq a`, `Hash a`, `Functor f`, `Traversable f` | Simple instance search; complex relations between independent types are expressed by explicit values |
| Several parameters | `Convert source target` | Sometimes both types have to be given explicitly because of ambiguity |
| Functional dependencies | The collection type uniquely determines the element type | Needs constraint improvement, checking of dependency consistency and search termination |
| Associated types | `Element collection`, the result type of an operation | The solver must normalize functions on types and solve equalities |
| General type families | Computation over types and more complex DSLs | The highest cost in the checker, module interfaces and diagnostics |

### Multiparameter classes and functional dependencies

```flick
class Collection c a | c -> a =
  empty : c
  insert : forall captures. a^captures -> (c -> c)^captures
```

The qualifier on insert keeps the value a in the partially applied function; the concrete collection type also keeps the captures of its elements.

The dependency `c -> a` reads: “a concrete collection type determines the unique element type”. For example, the heads `Collection UserSet User` and `Collection NameSet String` are possible.

```flick
empty_users : UserSet
let empty_users =
  empty
```

The expected `UserSet` also determines `User`. Without the dependency the method would have the type `Collection c a => c`, where `a` is not fixed by the result; a separate basis for choosing the dictionary unambiguously would be required.

The pair `Collection UserSet User` and `Collection UserSet String` violates the FD. The reverse direction is not promised: one `User` can be stored in different collections. After expanding aliases and normalizing built-in row expressions, coverage starts with the variables of the determining head arguments and closes over dependencies in the context and its instantiated superclasses. A dependency contributes its outputs only when all its inputs are covered; every determined head variable must belong to this least fixed point. Each head dependency is checked independently, so unrelated constraints and unseeded cycles do not help.

Context-assisted coverage permits capability lifting, not context-based selection. Instances are selected by their complete heads before their contexts are solved. Incompatible FD outputs are rejected, as are overlapping heads. For otherwise non-overlapping heads whose determining positions unify, the determined positions must already be equal under that determining substitution alone; contexts cannot justify additional output bindings. Context variables must still occur in the head, each constraint must be structurally smaller, and variable occurrences may not increase. These restrictions do not prove unbounded termination of FD improvement: every resolution and improvement retry consumes the configured step/depth budget, and exhaustion is `FLICK-C009`, never success.

A multiparameter class need not have a functional dependency. For `Convert String Email` and `Convert String UserId`, the dependency `input -> output` is precisely wrong: String allows both results.

The cost of FD is refining variables by constraints, checking the consistency of instances, coverage and search termination. [GHC functional dependencies](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/functional_dependencies.html) shows this mechanism on the relation of a collection to its element. For Schema/Codec a single wire format cannot be automatically assigned to one domain type.

### Associated types

Associated types are excluded from the first version. For comparison with the chosen FD, the same scenario would look like this:

```flick
class Collection c =
  type Item c

  insert : Item c -> c -> c
```

The instance for `UserSet` declares `Item UserSet = User`. The full signature of the generic method has the form `Collection c => Item c -> c -> c`.

This is convenient when an entity determines its related types: Repository — Entity and Key, Parser — Output, collection — Item. When `c` is unknown, the expression `Item c` may stay unevaluated. A unique `c` cannot be recovered from `Item c = User`.

The cost of associated types is a separate solver for equalities and normalization of type expressions. Even definitions only inside canonical instances add a new checker mechanism. The first version expresses such a relation through MPTC/FD or an explicit witness.

### Type families outside the first version

```text
type family Element container =
  | List a -> a
  | Option a -> a
```

Here the computation of types exists independently of a class dictionary and runtime methods. An unknown argument may leave `Element container` unevaluated. Rules would be needed for the applicability of equations, their compatibility, normalization and termination.

[GHC type families](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/type_families.html) include both associated definitions inside classes and standalone families. FD and associated types can be combined, but they are separate sources of complexity.

Full Schema deriving does not depend on accepting general type families: the compiler generates witnesses from the type definition and obtains the schemas of generic/HKT parameters explicitly.

## Coherence and alternative instances

Coherence means that one constraint gets a consistent implementation. For collections, changing `Eq`, `Hash` or `Ord` between insertion and lookup can break their behavior.

| Policy | Gain | Cost |
| - | - | - |
| One instance without overlap and orphans | Predictable semantics and compatibility of libraries | A nominal wrapper is needed for a foreign class and a foreign type |
| Allowed orphan instances | Convenient integration packages | Two libraries may offer incompatible instances |
| Overlap with priorities | A general implementation and specialized cases | Search gets more complex; adding a library can change the chosen implementation |
| Local implicit instances | Short notation for local configuration | Rules are needed for capturing a dictionary and passing values between scopes |
| Explicit dictionaries | Alternative behavior is visible in the API | More arguments; a collection must store the chosen Eq/Hash |

Accepted: instance heads, after expanding aliases, must not unify; their context is not used to resolve overlap. All context variables must be present in the head. Recursive class constraints must shrink structurally without increasing the number of occurrences of a variable; the superclass graph is acyclic. This is a conservative termination condition, with no analogue of `UndecidableInstances`. Exceeding the solver budget produces a diagnostic and never means successful resolution. `Eq a => Eq (List a)` is allowed. [GHC's instance termination rules](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/instances.html#instance-termination-rules) serve as a guide for this conservative restriction, not as a promise to support all GHC extensions.

### Overlap by example

Suppose two rules are declared:

```text
Show a => Show (List a)
Show (List Float)
```

Both fit `List Float`. Priority for the more specific instance looks convenient, but the compiler must decide which behavior an already compiled generic function gets and whether specialization may choose a different implementation.

[GHC describes the relation of overlap to specialization](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/instances.html#overlapping-instances). In the Flick optimizer, several dictionaries of one type require keeping the concretely chosen dictionary. Without overlap it is easier to obtain a direct method call and then apply inlining.

### An orphan instance and an external library

Let `Hash` be defined by the standard library and `ExternalId` by an SDK. An adapter package that owns neither the class nor the original type cannot declare a global `Hash ExternalId` under the no-orphan policy.

```flick
unboxed type LocalExternalId =
  | LocalExternalId ExternalId
```

Now the adapter owns a nominal type and can give it canonical instances. `unboxed` leaves the `ExternalId` itself at run time: the wrapper allocates nothing and goes into the SDK as is, while identity and instances stay with the nominal type. The transparent alias `type alias LocalExternalId = ExternalId` creates no new identity. Other paths are an instance at the owner of the class/type, or an explicitly passed dictionary.

Ownership is determined by the **package**: an instance can be declared by the package of the class or by the package of the external nominal constructor of the **first parameter of the head**. A transparent alias and a structural record do not create an owner. For MPTC, ownership of only a later parameter is not enough: the package of `LocalEmail` does not get the right to declare a foreign `Convert String LocalEmail`. For that, an explicit codec/dictionary, a wrapper of the first parameter or an instance at the owner of the class is needed. Such a restrictive anchor rules out competing owners of different parameters; the cost is additional wrappers in integrations. A binding does not change the identity of a foreign type.

### Local implicit instances

One could propose a scope inside which `Eq String` means a case-insensitive comparison. Then it must be defined which dictionary a closure captures and what a collection gets after leaving the scope. Lexical selection can be made deterministic, but the type name no longer determines a single behavior.

For an alternative sort an explicit comparator is enough. A hash collection must receive and store a consistent Eq/Hash pair; changing the surrounding instance does not change the semantics of an already populated collection.

Chosen: canonical implicit instances, nominal wrappers and explicit alternative dictionaries. Orphans, overlap with priorities and local implicit instances are forbidden in the first version. The set of instances is determined by the package dependency graph, not by the order or form of `import`; conflicts are diagnosed when the whole accessible graph is checked.

## Generalization in the presence of Ref

Generalization means that the type of a binding can be instantiated anew at each use. For `identity : forall a. a -> a`, two calls can independently choose `Float` and `String`. A single mutable cell must keep one content type. An explicit `forall` signature cannot make unsafe sharing of state acceptable.

```flick
invalid : Unit ->{} String
let invalid () =
  let cell = Ref.make None
  Ref.set cell (Some 42)
  match Ref.get cell with
  | Some value -> String.trim value
  | None -> ""
```

The program must be rejected: the write `Some 42` fixes the cell's content as `Option Int`, while `String.trim` requires `String`. If `cell` were given `forall a. Ref (Option a)`, the write and the read could choose different types for one object. The problem of polymorphic mutable cells and the relaxed value restriction is discussed in the [OCaml manual](https://ocaml.org/manual/5.4/polymorphism.html).

| Generalization rule | Implementation cost | Cost for the program author |
| - | - | - |
| Only syntactic values | The simplest conservative rule | Sometimes a wrapper function has to be added to express something safely |
| Provably pure construction of a value | Uses pure/runtime analysis and free variables of the environment | Fewer forced wrapper functions; requires truthful FFI contracts |
| Additional generalization by variance | Variance analysis is needed, including abstract types | Allows more safe programs; `Ref a` stays invariant |
| State effects and regions | A substantially more complex state system | Allows hiding local mutation behind a pure API |

**Provably pure constructions of values** are generalized, only over variables that are not free in the environment and not tied to local skolems, region/capture obligations or ambiguous constraints. The result of a runtime binding is not generalized even with `->{}`. A full signature does not cancel these checks. The relaxed value restriction by variance is not used in the first version; mutable containers are invariant. A trusted pure FFI must observe the same purity contract.

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

### Generalization in three small examples

**Syntactic values and provable purity.**

```flick
make_identity : Unit -> (a -> a)
let make_identity () =
  fun value -> value

identity_again : forall a. a -> a
let identity_again =
  make_identity ()
```

The right-hand side `make_identity ()` is a call, not a syntactic value. The strict rule “generalize only values” would reject the required polymorphic signature of `identity_again`; adding an explicit `forall` does not cancel the rule. The analysis of provable purity allows it: the call is pure, no state is allocated, and the variable `a` is not captured from the environment. Higher-rank inference is not needed here.

**A pure description of a computation and its runtime result.**

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

`task` creates a cold description. Therefore `fresh_cell` can be generalized: each `await` runs `Ref.make` anew. The resulting cells are again monomorphic. `await` does not mean caching the result and does not start a separate fiber.

```flick
capture_cell : Ref (Option a) -> Eff none none (Ref (Option a))
let capture_cell cell =
  task { cell }
```

Here repeated runs return the one captured cell. The type `a` is tied to the argument `cell`: a local description cannot generalize a variable that is free in the environment. Likewise memoization must not preserve the polymorphism of a fresh result if it turns it into shared state. The coldness of `Eff` by itself does not prove the absence of captures.

**What variance additionally allows.**

```flick
logged_none : Unit ->{Log; none} Option a
let logged_none () =
  Log.info "created an empty result"
  None

use_logged_none : Unit ->{Log; none} (Option Float, Option String)
let use_logged_none () =
  let empty = logged_none ()
  (empty, empty)
```

Under the conservative ban on generalizing runtime results, `empty` gets one unknown type `Option a`; it cannot be used in a pair at once as `Option Float` and `Option String`. The relaxed value restriction can generalize `a` in the covariant immutable `Option a`, keeping a single `Log.info` call. This is additional expressiveness, not a way to perform logging purely.

| Position of the variable | Example | Consequence for the relaxed value restriction |
| - | - | - |
| Covariant | Immutable `Option a` | A candidate for additional generalization |
| Invariant | `Ref a` | Shared mutable content is not generalized |
| Both input and output | `a -> a` | A covariance check alone is not enough |
| Abstract type | `Container a` from another module | Variance must be checked and preserved in the interface |

The rule of the first version is **provable purity + free variables of the environment + checking of lifetime and unambiguity of constraints**. The example `use_logged_none` above is deliberately rejected; it shows the cost of refusing variance-generalization. Additional generalization by variance is moved beyond the first version.

## Deriving and class laws

`deriving Eq, Ord, Hash, Show` creates class instances; full `deriving Schema` creates a runtime witness of the data. Deriving Schema covers records, ADTs, tuples, generics, recursive types, refinements and extension through explicit witnesses. It does not declare a type class of the same name and does not require a general language of type families. The detailed contract is in section 9.

Checking an instance confirms its type but does not prove the class laws. Generation of `Eq` and `Hash` must use consistent field semantics. User-written implementations require separate law checks; serialization and storage format are specified by an explicit codec, not by a single global instance.

## Monad and joining rows

```text
bind :
  forall c d m a b.
  Monad m => (m a)^c ->
  ((a -> (m b)^d)^d -> (m b)^(c + d))^c

bind_eff :
  forall c d r1 r2 e1 e2 a b.
  (Eff r1 e1 a)^c ->
  ((a -> (Eff r2 e2 b)^d)^d ->
    (Eff (r1 + r2) (e1 + e2) b)^(c + d))^c
```

Capture rows give upper bounds; the result keeps the captures of the original description and of the continuation. Partial application for now holds only the first argument.

The ordinary `Monad (Eff r e)` fixes `r` and `e`; their explicit or inferred widening to a common row is allowed. Direct runtime calls use a separate built-in sequencing with joining of rows, without going through the `Monad` instance.

The frontend computes the unions of requirements/errors for direct calls and preserves the captures of descriptions and callbacks. Indexed/graded abstractions are not part of the first version. A generic `do` elaborates to the `Monad` and `Alternative` methods of the Prelude directly, whatever names are in scope; with `Monad (Eff r e)` it keeps the fixed rows, which is why direct calls stay the main style for Eff.

**Decision for the type core:** MPTC/superclasses/FD with context-assisted coverage, structural context restrictions and budgeted resolution; coherent head-only selection; package ownership by the class or the first nominal parameter; no overlap/orphans/local implicits; fixed row primitives; generalization of only provably pure constructions. Higher-rank callbacks are checked against explicit nested `forall`; this feature weakens neither coherence nor the state restrictions.
