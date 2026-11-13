---
title: "Kinds and types"
---

The type checker runs after name resolution on every module without syntax
errors (see [Values of computations and their execution](../../spec/02-computations/), [The HKT type core and type classes](../../spec/03-types-and-classes/), [Schema and standard data types](../../spec/07-schema/)). It knows other modules only through their interfaces
([iface.md](../iface/)).

## Built-in types

The module `Prelude` declares them; every module sees its names without an
import, and a local definition of the same name hides them.

| Type | Kind |
| - | - |
| `Unit`, `Never`, `Bool`, `Int`, `Float`, `BigInt`, `String`, `Unknown`, `Ordering` | `Type` |
| `List`, `Option`, `Ref`, `Redacted`, `Schema` | `Type -> Type` |
| `Result`, `PureCodec` | `Type -> Type -> Type` |
| `ServiceTag`, `ServiceImpl` | `ServiceRow -> Type` |
| `Scope` (a service) | `Region -> Service` |
| `Eff` | `ServiceRow -> ErrorRow -> Type -> Type` |
| `Layer` | `ServiceRow -> ErrorRow -> ServiceRow -> Type` |
| `Codec` | `ServiceRow -> ServiceRow -> Type -> Type -> Type` |
| `Errors` | `ErrorRow -> Type` |

`Option a = None | Some a`, `Result e a = Err e | Ok a` and
`type error AppError = AppError { message : String; details : Option String }` are
ordinary declarations; `identity : a -> a` is a function. `from_result` turns a
`Result` into an expected error ([effects.md](../effects/)). Built-in modules
are used qualified without an import: `Ref.make`, `Ref.get`, `Ref.set`,
`Ref.modify`, `String.trim`, `Platform.stdout_line`, `Platform.stderr_line`,
`App.fail`, `Layer.succeed`, `Layer.merge`, `Layer.override`, the row
primitives of `Record`, and `Fiber.spawn` with the types `Fiber.Owner`,
`Fiber.Fiber` and `Failure` ([regions.md](../regions/)). The classes `Eq`, `Ord`, `Hash` and `Show` are in
`Prelude` too ([classes.md](../classes/)).

## Kinds

`Type`, `ServiceRow`, `ErrorRow`, `CaptureRow`, `Row k` (`Row Type` is the
kind of record fields), `Symbol`, `Region`, `Constraint`, the finite kinds of
`kind` declarations, and arrows between them. A type parameter may state its
kind; otherwise the kind follows from where the parameter is used, and what
nothing determines is `Type`. The parameters of a module's declarations are
inferred together, so declarations may use each other in any order.

```flick
type alias Box (f : Type -> Type) a = { value : f a }

kind Nullability = NonNull | Nullable

type Js (n : Nullability) a
```

The three effect-row kinds do not mix: a service row holds services, an error
row holds types declared with `error`, a capture row holds regions. `none` is
the empty row of any row kind and is not a type. `Eff Unit Unit a`, `Option`
used without its argument and `Float String` are `FLICK-T002`.

## Signatures

Every named function has its full signature right above its `let` (see [Direct effectful calls](../../spec/01-direct-calls/)): at
the top level, and for a function made by partial application. A local function
may omit it: its type is inferred, and without a signature it has one type, so
using it at a second type is `FLICK-T003`. A recursive local function needs one.
An instance method takes its signature from the class; repeating it is `FLICK-T059`.
A top-level function without one is `FLICK-T004`, with the inferred type
as a suggestion; types written in the `let` header are `FLICK-T005`; a
signature without the definition right below it is `FLICK-T006`. Values that
are not functions need no signature, and a local `let x : T = …` is a hint.

`todo` can stand for a whole signature or part of one while editing. It is an
error until replaced, so checking and building cannot succeed with it. The
editor offers the inferred replacement when it can verify a complete type;
unlike `_`, a type-level `todo` is never a warning-only hole.

New type variables of a signature are quantified over the whole signature
and their kinds are inferred. Inside its body, a nested signature that uses
the same name means the same type:

```flick
compose : (b -> c) -> (a -> b) -> a -> c
let compose f g x =
  apply : (a -> b) -> b
  let apply h = h x
  f (apply g)
```

A signature is checked, not trusted: the body must work for every type the
caller may choose (`FLICK-T008` when it needs one concrete type).

A `_` in a signature or annotation is a hole: the checker infers that part
and reports it (`FLICK-T024`), with a fix that writes the inferred type in.
A hole is for drafting; a type declaration cannot have one.

```flick
swap : (Float, String) -> _
let swap (a, b) = (b, a)
```

## Functions and higher rank

`a -> b`, `a ->{r; e} b` and `a ->{} b` are three different types; an empty
row never makes a runtime arrow pure (see [Values of computations and their execution](../../spec/02-computations/)). A lambda takes each arrow of the
type it is checked against. Which rows a body's calls need, and where a
runtime call may run at all, is the subject of [effects.md](../effects/).

A `forall` may appear inside a signature's type. Checking then flows from the
signature into the argument, so one callback can be used at several types:

```flick
apply_twice :
  (forall a. a -> a) -> (Float, String)
let apply_twice transform =
  (transform 42, transform "hello")
```

Instantiation is predicative: a type variable never stands for a
polymorphic type. A variable bound by a `forall` must not leave it, e.g. by
being stored in a cell created outside the callback (`FLICK-T007`).

## Records

Records are exact and structural (see [The HKT type core and type classes](../../spec/03-types-and-classes/)). `type alias User = { … }` is a transparent
alias, and the order of fields does not matter. A record with extra fields
does not pass for a closed record type; a signature accepts more fields with
an open row:

```flick
name : forall fields. { name : String | fields } -> String
let name value = value.name
```

`{ r with f = v }` replaces existing fields by values of the same type. A
label given twice in a record type, literal, update or pattern is
`FLICK-T014`.

## Patterns

A `match` must cover every value; `FLICK-T009` shows one that no arm matches.
Arms with a `when` guard cover nothing. An arm that can never run is the
warning `FLICK-T010`. A constructor pattern takes exactly the constructor's
arguments (`FLICK-T018`). Patterns in `let` and in parameters must match every
value. A record pattern lists some of the fields.

A list pattern `[a, b]` matches only lists of that length, and `[a, b, ...rest]`
only lists of at least that length, so `[] | [x, ...rest]` covers every list.
A list can have any length, so a list pattern in `let` is an error unless it is
`[...all]`: use `match`, or a tuple when the length is fixed. The error of a
local `let` offers both as fixes: `match` takes the rest of the block as the
arm of the pattern and adds an arm `-> todo` for what it misses.

`let assert p = e` binds `p` when it matches, and otherwise fails like `todo`
with its place, the message of `as "…"` and the value: `let assert [x, ..._] =
xs as "no items"`. It belongs in a block, not at the top level of a module
(`FLICK-T026`); with a pattern that matches every value it is the warning
`FLICK-T025`. `[lint] no-let-assert` in `flick.toml` makes every `let assert`
an error (`FLICK-T027`).

## GADTs

A constructor may name the type it builds after `:`. Its type variables are
its own, and those of its fields that the result does not mention are
existential: a value hides their types. A constructor without `: R` builds
`T a…` as usual, and both kinds mix in one type. [GADTs](../gadts/) walks
through what follows with examples.

```flick
type Expr a =
  | IntLit Int : Expr Int
  | BoolLit Bool : Expr Bool
  | Add (Expr Int) (Expr Int) : Expr Int
  | If (Expr Bool) (Expr a) (Expr a) : Expr a
  | Apply (Expr (b -> a)) (Expr b) : Expr a
  | Lit a

eval : forall a. Expr a -> a
let rec eval e =
  match e with
  | IntLit n -> n
  | BoolLit b -> b
  | Add x y -> eval x + eval y
  | If c t f -> if eval c then eval t else eval f
  | Apply f x -> (eval f) (eval x)
  | Lit x -> x
```

A branch learns what the type of the matched value is: in the `IntLit` branch
`a` is `Int`, so `n` is a result of type `a`, and an editor shows a value of
type `a` there as `Int (= a)`. The result of a constructor is its own type
applied to all of the type's parameters (`FLICK-T056`); an argument is a type,
a member of a `kind` or a `Symbol`, and parameters of regions and record rows are
passed as variables. [Service, error and capture row indices](../row-indices/)
can be fixed or related by the constructor.

A `match` needs no arm for a constructor that builds no value of the matched
type, and an arm for one is the warning `FLICK-T010`:

```flick
# type Expr a =
#   | IntLit Int : Expr Int
#   | BoolLit Bool : Expr Bool
#   | Add (Expr Int) (Expr Int) : Expr Int
size : Expr Int -> Int
let rec size e =
  match e with
  | IntLit _ -> 1
  | Add x y -> size x + size y
```

What one constructor tells holds across the matched value: on `(x, y)` of type
`(Ty a, Ty a)`, a pair of different constructors of `Ty` needs no arm. A type
the constructor hides is not followed.

The checker learns from a branch only where the types are known
(`FLICK-T057`): the function has a complete signature, the type of the matched
value is known where it is matched, and nothing from outside the branch gets
its type from what the branch learned; such a type needs an annotation. A
`let` in the branch is not generalized. An existential type cannot leave its
branch (`FLICK-T007`). What a field holds, the value holds, so a scoped
resource hidden in a value cannot leave its scope either. Only an arm of a
`match` refines or hides types: the pattern of a `let`, of a parameter or an
alternative of an or-pattern does not (`FLICK-T058`). Regions retain their
identities; [row indices](../row-indices/) can be refined.

`TypeEq a b` of the core module `TypeEq` is evidence that two types are one:
`Refl : TypeEq a a`, and `TypeEq.cast : TypeEq a b -> a -> b`.

```flick
import TypeEq exposing (TypeEq(..))

type Ty a =
  | TInt : Ty Int
  | TStr : Ty String

equal_types : forall a b. Ty a -> Ty b -> Option (TypeEq a b)
let equal_types a b =
  match (a, b) with
  | (TInt, TInt) -> Some Refl
  | (TStr, TStr) -> Some Refl
  | _ -> None

coerce : forall a b. Ty a -> Ty b -> a -> Option b
let coerce a b x =
  match equal_types a b with
  | Some eq -> Some (TypeEq.cast eq x)
  | None -> None
```

TypeScript has no refinement: in a `.d.ts` file each constructor of a GADT is
a case of every instance of its type, and a type it hides is `unknown`.

## Numbers and operators

See [numbers and operators](../operators/#numbers-and-operators).

## Blocks

See [bindings and local execution](../bindings/#blocks).

## Variables

See [bindings and local execution](../bindings/#variables).

## Local loops

See [bindings and local execution](../bindings/#local-loops).

## Recursion

See [bindings and local execution](../bindings/#recursion).

## Generalization

See [bindings and local execution](../bindings/#generalization).

## Not checked yet

See [bindings and local execution](../bindings/#not-checked-yet).
