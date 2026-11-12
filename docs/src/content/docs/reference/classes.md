---
title: "Classes and instances"
---

Classes are resolved by the checker after types are inferred: every
constraint gets exactly one dictionary, chosen by the instances the module
can see. Service operations are not class methods ([effects.md](../effects/)).

## Classes

```flick
class Show a =
  show : a -> String

class Eq a => Hash a =
  hash : a -> Int

class Collection c a | c -> a =
  empty : c
  insert : a -> c -> c

class Functor (f : Type -> Type) =
  map : (a -> b) -> f a -> f b
```

A class has one or more parameters of any kind, superclasses before `=>`
and functional dependencies after `|`: `c -> a` says the types at `c`
determine the types at `a`. A method's type is
`forall params own. C params => …`. The superclasses must not lead back to
the class (`FLICK-C008`). A class without methods only relates its
parameters and has no `=`.

`Prelude` declares `Eq`, `Ord` (`compare : a -> a -> Ordering`, with
`type Ordering = LT | EQ | GT`), `Hash` and `Show`, with instances for the
built-in types, `List`, `Option` and `Result`; `Ref` has `Eq` (the same
cell) but no `Hash`. `Display` (`display : a -> String`) is what a hole
`\{e}` of a string inserts, with instances for `String`, `Int`, `Float` and
`Bool`. `Into a b` (`into : a -> b`) names a conversion that loses nothing:
`Into Int Float` and `Into Int BigInt`. It has no functional dependency, so
the type `into` gives is the one its place expects; without one it is
`FLICK-C002`. A program declares `Into` for its own types like any class.

## Implicit conversions

`@implicit` before an instance opts it into automatic conversion: where a
value of ground nominal type `A` meets a place expecting ground nominal type
`B`, exactly one visible implicit instance must convert `A` to `B`.

```flick
unboxed type Meters = Meters Float
type Feet = Feet Float

@implicit
instance Into Meters Feet =
  let into (Meters n) = Feet (n * 3.0)

feet : Feet
let feet = Meters 2.0

read : Feet -> Float
let read (Feet n) = n

let length = read (Meters 3.0)
```

The class must have exactly two parameters and one method of type `a -> b`,
without effects or additional constraints. The instance must have no context
or type variables, and its two types must differ (`FLICK-C014`).
`@implicit` takes no argument and belongs only before an instance (`FLICK-S014`).
The class need not be `Into`.

Conversion takes one step, never a chain. It does not infer an unknown type,
convert functions, records, tuples, rows or captured/bounded types, or change
ordinary successful type checking. Nominal applications must themselves have
ground nominal arguments. Two matching instances, even of different classes,
leave the type mismatch unresolved. Arguments, annotated bindings, return
positions, branches and list elements can receive a conversion when their
expected type is known. Explicit method calls such as `into x` still work.
The core's numeric `Into` instances are **not** implicit.

## Instances

```flick
# class Show a =
#   show : a -> String
# show_items : Show a => List a -> String
# let show_items items = String.join ", " (List.map show items)
#
instance Show a => Show (List a) =
  let show items = "[\{show_items items}]"
```

Each method of the class is defined once with the class signature, the
instance's types in place of the class parameters. Its own signature is
optional: without one the method has exactly that type; a written one may be
more polymorphic, but may not require a constraint the class signature does
not (`FLICK-C010`). A missing or unknown method is `FLICK-C011`. The superclasses
must hold for the instance's types, given its context (`FLICK-C001`).

A method may have a default: its signature and a `let` right below it in
the class body. An instance that does not define the method takes the
default, which sees the instance's dictionary like any function constrained
by the class; an instance's own definition replaces it. A method without a
default is abstract, and an instance without it is `FLICK-C011`, as is a
`let` in a class body whose name is not a method of the class.

```flick
class Pretty a =
  pretty : a -> String
  verbose : a -> String
  let verbose x = "<\{pretty x}>"

instance Pretty Float =
  pretty : Float -> String
  let pretty n = show n
```

A default may need more than the class: the class body then repeats the
method's signature with those constraints before it, a default signature,
and the default is checked against it. An instance that takes the default
must satisfy them, for its types (`FLICK-C001` on the instance names the one
it lacks); one that defines the method needs none of them. The default
signature has the method's type exactly and constrains only the class's
parameters and variables of its own, as `rep` in
`(Generic a rep, GDefault rep) => a`: the method itself may have no
constraints of its own, and a method has at most one (`FLICK-C010`). A
default signature without its `let` is `FLICK-C011`.

```flick
class Describe a =
  describe : a -> String
  describe : Show a => a -> String
  let describe x = "<\{show x}>"

instance Describe Float   // takes the default: `Show Float` holds
```

Instances are canonical. The checker rejects an instance when:

| Rule | Code |
| - | - |
| its head unifies with another instance's, after aliases are expanded; the context does not choose | `FLICK-C003` |
| determining arguments unify but the determined arguments disagree, or a non-overlapping pair needs further output bindings to agree | `FLICK-C004` |
| a determined variable is not covered by the determining head arguments and the context's dependencies | `FLICK-C005` |
| neither the class nor the nominal type at the head of its first type belongs to this package | `FLICK-C006` |
| a context variable is not in the head, or a context constraint is not smaller than the head or repeats a variable more often | `FLICK-C007` |
| it is for a row primitive, for `Eq`, `Ord`, `Hash`, `Show` of a record or tuple, or for the closed `Numeric` or `Iterable` | `FLICK-C012` |
| it has `@implicit`, but its class or instance head does not meet the conversion rules | `FLICK-C014` |

Coverage starts with the variables in a dependency's determining head
arguments. The checker repeatedly adds variables determined by the instance
context and its instantiated superclasses, only when all inputs of that
dependency are covered. Each dependency of the instance head is checked
independently; unrelated constraints and unseeded cycles add nothing.
Aliases are expanded first. This admits ordinary capability lifting:

```flick
# class MonadState (m : Type -> Type) s | m -> s
# type ReaderT r (m : Type -> Type) a = ReaderT (r -> m a)
#
instance MonadState m s => MonadState (ReaderT r m) s
```

The head determines `m`, and the context's `m -> s` determines `s`.
Context-assisted coverage does not allow context-based instance selection.
Heads alone must be coherent: for non-overlapping heads whose determining
arguments unify, their outputs must be equal under that substitution alone.

Ownership goes by package: a project's source and test modules are one
package, each dependency another, the built-in modules a third. A transparent
alias or a record does not give ownership; a nominal wrapper does:

```flick
# import Sdk exposing (ExternalId, hash_external)
#
unboxed type LocalExternalId = LocalExternalId ExternalId

instance Hash LocalExternalId =
  let hash id =
    match id with
    | LocalExternalId raw -> hash_external raw
#
# instance Eq LocalExternalId =
#   let eq (LocalExternalId a) (LocalExternalId b) = hash_external a == hash_external b
```

`unboxed` keeps the `ExternalId` itself at run time: the wrapper costs
nothing and passes to the SDK as it is, while the instances belong to the
nominal type.

A module sees the instances of every module it depends on, directly or not,
however it imports them; the order and form of `import` change nothing.
Two modules that do not depend on each other cannot see each other's
instances: a module that sees both reports an overlap between them
(`FLICK-C003`, `FLICK-C004`) at its `module` line.

## Resolution

A constraint is solved, in order, by an assumption of the enclosing signature,
instance or higher-rank `forall` (or one of their superclasses), by a row
primitive, by the structural instance of a record or tuple, or by the one
instance whose head matches it. A functional dependency fixes unknown types:
with `Collection UserSet User`, `empty : UserSet` needs no annotation for the
element. Constraints are solved when the definition is checked; a value that
is generalized keeps the variables of unsolved constraints monomorphic.

| Failure | Code |
| - | - |
| no instance applies, or the signature does not assume the constraint; the message lists the instances of the class | `FLICK-C001` |
| a type in the constraint is not known, so no instance can be chosen | `FLICK-C002` |
| resolution exceeds its budget; the message shows the chain of constraints | `FLICK-C009` |

The budget is per definition: `solver-steps` (100 000) and `solver-depth`
(256 nested constraints) in `[build]` of `flick.toml`. Structural context
restrictions still apply, but context-assisted FD improvement is not a proof
of unbounded termination. Every resolution and improvement retry consumes
the budget; running out is never a success.

## Records and tuples

Records and tuples have compiler-owned `Eq`, `Ord`, `Hash` and `Show` when
each field has the instance; two transparent aliases of one form share them
and cannot have their own. An open record needs the constraint on the whole
record as an assumption.

## Row primitives

The module `Record` declares classes the compiler solves from the fields of
known records; a program cannot declare instances of them. A label at the
type level is a string: `"name"`, of kind `Symbol`.

| Primitive | Holds when |
| - | - |
| `HasField l r a` | `r` has field `l` of type `a` (`l r -> a`) |
| `Lacks l r` | `r` has no field `l` |
| `Remove l r out` | `out` is `r` without field `l` (`l r -> out`) |
| `Project r out` | every field of `out` is a field of `r` of the same type |
| `UnionDisjoint a b out` | no label is in both, `out` has the fields of both (`a b -> out`) |
| `Map f r out` | `out` has the labels of `r`, each field type `t` becoming `f t` (`f r -> out`) |
| `Labels r` | the labels of `r` are known |

`Record.Field "x" T` is `{ x : T }`. Functions: `Record.label : Label l`
(used as `(Record.label : Record.Label "x")`), `Record.get`, `Record.extend`
(needs `Lacks`), `Record.remove`, `Record.project`, `Record.merge_disjoint`
(needs `UnionDisjoint`) and `Record.labels` (in canonical, sorted order).
Extending an open record needs `Lacks` for its row in the signature:

```flick
tag : Record.Lacks "tag" { id : Float | r } => { id : Float | r } -> { id : Float; tag : String | r }
let tag value = Record.extend (Record.label : Record.Label "tag") "t" value
```

## Call sites

The core class `CallSite` of the module `CallSite` is the place of a use:
`CallSite.place` is `Module:line` and `CallSite.definition` the
`Module.name` of the top-level definition around it, or `Module`. The
checker gives it, never an instance: a function with `CallSite =>` in its
signature passes on its caller's, and any other use gets its own place.
`Atom.set` names the place of the write in a debug log, a serializable
atom takes its key from the definition, and `todo`, `echo` and `let
assert` name their place this way.

```flick
# import CallSite exposing (CallSite)
#
/// The place of the caller of `traced`.
traced : CallSite => String -> String
let traced label = "\{label} at \{CallSite.place}"
```

A program declares no instance of it (`FLICK-C012`). In the context of an
instance or the signature of a default method, it is only a given.

## Deriving

```flick
type Tree a = Leaf | Node (Tree a) a (Tree a)
  deriving Eq, Show
```

`deriving` makes instances of `Eq`, `Ord`, `Hash` and `Show` for an ADT, an
error, a refined type or an opaque alias, requiring the class of each type
parameter of kind `Type`. Every field needs an instance; `Hash` and `Ord`
are derived only together with `Eq`, so all of them use the same fields. A
transparent alias already has its form's instances. Failures are
`FLICK-C013`.

A class marked `@derive` is derived by the plugin of its package, or of the
package the attribute names, `@derive std` (see
[plugins](../plugins/#derives)). `deriving C { … }` passes the derive its
arguments, and `deriving instance C (M.T a)` derives for a type of another
module of the package. `Cli.Value` is such a class. Any other class is
`FLICK-C013`: the class has no derive.

```flick
@derive
class Arbitrary a =
  arbitrary : a

type Point = Point Float Float
  deriving Arbitrary

deriving instance Arbitrary (Shapes.Shape)
```

`deriving Generic` (module `Generic`) gives a type its representation, so a
class can be written once for every type over it. The instance is
`Generic T rep`, with `to_rep : T -> rep` and `from_rep : rep -> T`:

| `T` | `rep` |
| - | - |
| `type Shape = Circle Float \| Dot` | `Sum (Con "Circle" (Field "_0" Float, Unit)) (Con "Dot" Unit)` |
| `type Named = Named { width : Float; label : String }` | `Con "Named" (Field "label" String, (Field "width" Float, Unit))` |

Each constructor is a `Con` of its name holding its fields in pairs ended by
`Unit`: numbered `_0`, `_1`, … or, when its one field is a record, labelled
with the record's labels in sorted order, as `Named` shows; several
are nested `Sum`s (`Inl`, `Inr`) in declaration order. `Con` and `Field` are
unboxed: the representation costs one `Sum` per constructor but the last
and a pair per field. A type without constructors has none (`FLICK-C013`).

```flick
# import Generic exposing (Generic(..), Field(..))
#
class GCount rep =
  gcount : rep -> Int

instance GCount Unit =
  let gcount _ = 0

instance GCount rest => GCount (Field l a, rest) =
  let gcount (_, rest) = 1 + gcount rest

count : (Generic a rep, GCount rep) => a -> Int
let count x = gcount (to_rep x)
```

A class of one parameter with a default for each method is derived too: the
instance takes every default, requiring the class of each type parameter of
kind `Type`. A default signature that needs `Generic` gives the type its
`Generic` instance as well, so a class written once over the representation
derives for every type:

```flick
# import Generic exposing (Generic(..), Sum(..), Con(..), Field(..))
#
# class GDefault rep =
#   gdefault : rep
#
# instance GDefault Unit =
#   let gdefault = ()
#
# instance (Default a, GDefault rest) => GDefault (Field l a, rest) =
#   let gdefault = (Field default, gdefault)
#
# instance GDefault fields => GDefault (Con n fields) =
#   let gdefault = Con gdefault
#
class Default a =
  default : a
  default : (Generic a rep, GDefault rep) => a
  let default = from_rep gdefault

type Pair a = Pair a a
  deriving Default    // instance Default a => Default (Pair a)
```

Where a type lacks what a default needs, as a `GDefault` of one of its
fields, the clause is `FLICK-C013`, as is a class with a method without a
default or with more than one parameter.


A constructor of a GADT that hides no type and holds no dictionary is
derived like any other, with each field at the type the constructor builds:
`IntLit Int : Expr Int` shows its `Int` with `Show Int`. One that hides the
type of a field or holds a dictionary has no derived instance (`FLICK-C013`),
nor does a GADT have `Generic`; `Schema`, `Url` and `Form` refuse one with
their own codes.
## Dictionaries

Each solved constraint records its dictionary at the place that required it:
an instance applied to the dictionaries of its context, an assumption, a
superclass of another dictionary, a structural instance by field, or a
primitive. Code generation passes them as arguments.

### Dictionaries in values

A constructor of a GADT may hold dictionaries: the constraints before it,
`C =>`, are solved where a value is built (`FLICK-C001` there), and the value
keeps their dictionaries after its fields. An arm of a `match` on the
constructor has them as assumptions, for the types the constructor hides
too:

```flick
type Showable =
  | Show b => Showable b : Showable

describe : Showable -> String
let describe s =
  match s with
  | Showable x -> show x

shown : List String
let shown = List.map describe [Showable 1, Showable "two", Showable [3.5]]
```

Only an arm of a `match` takes them out (`FLICK-T058` for the pattern of a
`let` or a parameter). `==` compares the fields of such values, not their
dictionaries; in a `.d.ts` file a held dictionary is no field.
