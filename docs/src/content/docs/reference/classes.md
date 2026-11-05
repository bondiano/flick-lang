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
cell) but no `Hash`.

## Instances

```flick
instance Show a => Show (List a) =
  let show items = "[" ++ show_items items ++ "]"
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
  let verbose x = "<" ++ pretty x ++ ">"

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
  let describe x = "<" ++ show x ++ ">"

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

Coverage starts with the variables in a dependency's determining head
arguments. The checker repeatedly adds variables determined by the instance
context and its instantiated superclasses, only when all inputs of that
dependency are covered. Each dependency of the instance head is checked
independently; unrelated constraints and unseeded cycles add nothing.
Aliases are expanded first. This admits ordinary capability lifting:

```flick
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
unboxed type LocalExternalId = LocalExternalId ExternalId

instance Hash LocalExternalId =
  let hash id =
    match id with
    | LocalExternalId raw -> hash_external raw
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

## Dictionaries

Each solved constraint records its dictionary at the place that required it:
an instance applied to the dictionaries of its context, an assumption, a
superclass of another dictionary, a structural instance by field, or a
primitive. Code generation passes them as arguments.
