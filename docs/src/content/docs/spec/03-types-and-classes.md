---
title: "Types and classes"
source_hash: 3ff9e97e8c7d2377
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
- GADTs: a constructor names the type it builds (`| IntLit Int : Expr Int`) and may hide the types of its fields; a `match` branch learns the type of the matched value under a complete signature, and exhaustiveness asks no branch for an impossible constructor, across all parts of the matched value. Indices include types, finite kinds, `Symbol` and service, error and capture rows. Branch-local row equations constrain every residual; region identities and record rows are not refined ([row indices](../../reference/row-indices/)).
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

[Continue in Records and instances](../03b-records-and-instances/#record-structurality).

### Records, instances and FFI

[Continue in Records and instances](../03b-records-and-instances/#records-instances-and-ffi).

## Classes and dependencies between types

[Continue in Records and instances](../03b-records-and-instances/#classes-and-dependencies-between-types).

### Functional dependencies

[Continue in Records and instances](../03b-records-and-instances/#functional-dependencies).

## Coherence and ownership

[Continue in Records and instances](../03b-records-and-instances/#coherence-and-ownership).

### An external type

[Continue in Records and instances](../03b-records-and-instances/#an-external-type).

## Generalization in the presence of Ref

[Continue in Generalization and monads](../03c-generalization-and-monads/#generalization-in-the-presence-of-ref).

### Pure calls and cold descriptions

[Continue in Generalization and monads](../03c-generalization-and-monads/#pure-calls-and-cold-descriptions).

## Deriving and class laws

[Continue in Generalization and monads](../03c-generalization-and-monads/#deriving-and-class-laws).

## Monad and joining rows

[Continue in Generalization and monads](../03c-generalization-and-monads/#monad-and-joining-rows).
