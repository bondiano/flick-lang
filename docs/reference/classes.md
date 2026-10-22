# Classes and instances

Classes are resolved by the checker after types are inferred: every
constraint gets exactly one dictionary, chosen by the instances the module
can see. Service operations are not class methods ([effects.md](effects.md)).

## Classes

```flick
class Show a =
  show : a -> String

class Eq a => Hash a =
  hash : a -> Number

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

instance Pretty Number =
  pretty : Number -> String
  let pretty n = show n
```

Instances are canonical. The checker rejects an instance when:

| Rule | Code |
| - | - |
| its head unifies with another instance's, after aliases are expanded; the context does not choose | `FLICK-C003` |
| two heads agree on the determining types of a dependency but not on the determined ones | `FLICK-C004` |
| a variable of the determined part does not appear in the determining part (strict coverage) | `FLICK-C005` |
| neither the class nor the nominal type at the head of its first type belongs to this package | `FLICK-C006` |
| a context variable is not in the head, or a context constraint is not smaller than the head or repeats a variable more often | `FLICK-C007` |
| it is for a row primitive, or for `Eq`, `Ord`, `Hash`, `Show` of a record or tuple | `FLICK-C012` |

Ownership goes by package: a project's source and test modules are one
package, each dependency another, the built-in modules a third. A transparent
alias or a record does not give ownership; a nominal wrapper does:

```flick
type LocalExternalId = LocalExternalId ExternalId

instance Hash LocalExternalId =
  let hash id =
    match id with
    | LocalExternalId raw -> hash_external raw
```

A module sees the instances of every module it depends on, directly or not,
however it imports them; the order and form of `import` change nothing.

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
(256 nested constraints) in `[build]` of `flick.toml`. Running out is never a
success.

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
tag : Record.Lacks "tag" { id : Number | r } => { id : Number | r } -> { id : Number; tag : String | r }
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

## Dictionaries

Each solved constraint records its dictionary at the place that required it:
an instance applied to the dictionaries of its context, an assumption, a
superclass of another dictionary, a structural instance by field, or a
primitive. Code generation passes them as arguments.
