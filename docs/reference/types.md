# Kinds and types

The type checker runs after name resolution on every module without syntax
errors (§4, §5, §9). It knows other modules only through their interfaces
([iface.md](iface.md)).

## Built-in types

The module `Prelude` declares them; every module sees its names without an
import, and a local definition of the same name hides them.

| Type | Kind |
| - | - |
| `Unit`, `Never`, `Bool`, `Number`, `BigInt`, `String`, `Unknown` | `Type` |
| `List`, `Option`, `Ref`, `Redacted`, `Schema` | `Type -> Type` |
| `Result`, `PureCodec` | `Type -> Type -> Type` |
| `ServiceTag`, `ServiceImpl` | `ServiceRow -> Type` |
| `Eff` | `ServiceRow -> ErrorRow -> Type -> Type` |
| `Layer` | `ServiceRow -> ErrorRow -> ServiceRow -> Type` |
| `Codec` | `ServiceRow -> ServiceRow -> Type -> Type -> Type` |
| `Errors` | `ErrorRow -> Type` |

`Option a = None | Some a`, `Result e a = Err e | Ok a` and
`type error AppError = AppError { message : String; details : Option String }` are
ordinary declarations; `identity : a -> a` is a function. `from_result` turns a
`Result` into an expected error ([effects.md](effects.md)). Built-in modules
are used qualified without an import: `Ref.make`, `Ref.get`, `Ref.set`,
`Ref.modify`, `String.trim`, `Platform.stdout_line`, `Platform.stderr_line`,
`App.fail`, `Layer.succeed`, `Layer.merge`, `Layer.override`.

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
used without its argument and `Number String` are `FLICK-T002`.

## Signatures

Every named function has its full signature right above its `let` (§3): at
the top level, locally, in instances, and for a function made by partial
application. A function without one is `FLICK-T004`, with the inferred type
as a suggestion; types written in the `let` header are `FLICK-T005`; a
signature without the definition right below it is `FLICK-T006`. Values that
are not functions need no signature, and a local `let x : T = …` is a hint.

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

## Functions and higher rank

`a -> b`, `a ->{r; e} b` and `a ->{} b` are three different types; an empty
row never makes a runtime arrow pure (§4). A lambda takes each arrow of the
type it is checked against. Which rows a body's calls need, and where a
runtime call may run at all, is the subject of [effects.md](effects.md).

A `forall` may appear inside a signature's type. Checking then flows from the
signature into the argument, so one callback can be used at several types:

```flick
apply_twice :
  (forall a. a -> a) -> (Number, String)
let apply_twice transform =
  (transform 42, transform "hello")
```

Instantiation is predicative: a type variable never stands for a
polymorphic type. A variable bound by a `forall` must not leave it, e.g. by
being stored in a cell created outside the callback (`FLICK-T007`).

## Records

Records are exact and structural (§5). `type alias User = { … }` is a transparent
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

## Numbers and operators

There are no implicit conversions (§9).

| Operators | Operands | Result |
| - | - | - |
| `+ - * / % **`, prefix `-` | both `Number` or both `BigInt` | the same type |
| `++` | both `String` or both `List a` | the same type |
| `< <= > >=` | both `Number`, `BigInt` or `String` | `Bool` |
| `== !=` | the same type | `Bool` |
| `&& \|\|`, `not` | `Bool` | `Bool` |
| `x \|> f` | `f x` | the result of `f` |

`1 + 1n` and `"a" ++ 1` are `FLICK-T003`. An operand nothing else determines
is `Number` (`String` for `++`).

## Blocks

Every statement before the last must have type `Unit`; another value is
dropped only with `ignore` (`FLICK-T011`, with that fix). The last statement
is the value of the block and must be an expression (`FLICK-T019`).

## Recursion

Top-level declarations may use each other in any order, but recursion is
explicit: a function that uses itself is `let rec`, and functions that use
each other are defined in one `rec { … }` group (`FLICK-T017`). Only functions
recurse; a value that would be evaluated while it is defined is `FLICK-T012`.

A recursive function may call itself at another type only when its
signature starts with a `forall` that binds all its variables; otherwise
that call is `FLICK-T015`.

```flick
type Nested a = Flat a | Nest (Nested (List a))

depth : forall a. Nested a -> Number
let rec depth value =
  match value with
  | Flat _ -> 0
  | Nest inner -> 1 + depth inner
```

## Generalization

A binding is polymorphic only when it is built purely: no runtime call runs
while it is defined (§5 "Generalization при наличии Ref"). Lambdas and `task`
bodies are deferred and count as pure. A polymorphic signature over a runtime
result is `FLICK-T013`, and so is a top-level value whose type stays unknown.
Type variables of an enclosing signature, or of a value already in scope,
are never generalized, and mutable containers are invariant.

```flick
make_empty : forall a. Unit ->{} Ref (Option a)
let make_empty () =
  Ref.make None

fresh_cell : forall a. Eff none none (Ref (Option a))
let fresh_cell =
  task { Ref.make None }
```

## Not checked yet

| Construct | Today |
| - | - |
| class constraints `C a =>` | kind-checked, not resolved; instance methods are checked against their own signatures |
| capture qualifiers `T^c` | kind-checked, then dropped |
| `sql`, `ui`, `codec`, `schema` blocks | any type |
| `scope s` handle | any type |
