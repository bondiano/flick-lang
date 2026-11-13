---
title: "Generalization and monads"
source_hash: 7e98f39218fcb96c
---

## Generalization in the presence of Ref

Generalization means that the type of a binding can be instantiated anew at each use. For `identity : forall a. a -> a`, two calls can independently choose `Float` and `String`. A single mutable cell must keep one content type. An explicit `forall` signature cannot make unsafe sharing of state acceptable.

```flick fail T003
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

A `let mut` variable is state of one function, not a cell: an assignment is not a runtime step, and a pure function may assign its variables. It obeys the same rule as a cell: its type is fixed where it is declared, is never generalized, and every assigned value is checked against it, so a write and a read cannot choose different types and a `scope` cannot give the variable its region. No function, task, lazy sequence or arm of an operation handler inside the function reads or assigns the variable, which keeps the function pure from the outside ([Variables](../../reference/types/#variables)).

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
