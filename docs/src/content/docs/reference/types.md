---
title: "Kinds and types"
---

The type checker runs after name resolution on every module without syntax
errors (see [Values of computations and their execution](../../spec/04-computations/), [The HKT type core and type classes](../../spec/05-types-and-classes/), [Schema and standard data types](../../spec/09-schema/)). It knows other modules only through their interfaces
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

Every named function has its full signature right above its `let` (see [Direct effectful calls](../../spec/03-direct-calls/)): at
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

A `_` in a signature or annotation is a hole: the checker infers that part
and reports it (`FLICK-T024`), with a fix that writes the inferred type in.
A hole is for drafting; a type declaration cannot have one.

```flick
swap : (Float, String) -> _
let swap (a, b) = (b, a)
```

## Functions and higher rank

`a -> b`, `a ->{r; e} b` and `a ->{} b` are three different types; an empty
row never makes a runtime arrow pure (see [Values of computations and their execution](../../spec/04-computations/)). A lambda takes each arrow of the
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

Records are exact and structural (see [The HKT type core and type classes](../../spec/05-types-and-classes/)). `type alias User = { … }` is a transparent
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

## Numbers and operators

There are three numeric types, and no implicit conversions between them (see
[Schema and standard data types](../../spec/09-schema/)):

| Type | Values | Arithmetic |
| - | - | - |
| `Int` | 32-bit signed integers | wraps around: `2147483647 + 1 == -2147483648`; `/` drops the fraction (`7 / 2 == 3`, `-7 / 2 == -3`); `/` and `%` by zero are `0` |
| `Float` | IEEE 754 binary64, a JS number | `0.0 / 0.0` is NaN, `1.0 / 0.0` is `Infinity`; `==` is SameValueZero |
| `BigInt` | integers of any size | `/` drops the fraction; `/` and `%` by zero are `0n` |

The same rules hold on every backend. `Int.checked_add`, `Int.checked_div`
and the others return `Result Int.ArithmeticError Int` instead of wrapping or
giving `0`; `Int.unsafe_div` and `Int.unsafe_rem` are a defect on a zero
divisor. `Int.to_float`, `Int.from_float` (`None` unless whole and in range)
and `Int.truncate` convert.

A number literal written as an integer (`3`, `0xFF`, `1_000`) is of any
numeric type its use asks for, and `Int` when nothing does: `let ratio :
Float = 3` and `3 + 0.5` are `Float`, `3 + 4n` is `BigInt`. A literal with a
fraction or an exponent (`3.0`, `1e3`) is a `Float`. An integer literal out
of the range of its type is `FLICK-T003`. When a class constraint is all that
decides a literal's type, it is the first of `Int`, `Float`, `BigInt` with a
matching instance.

| Operators | Operands | Result |
| - | - | - |
| `+ - * / % **`, prefix `-` | both `Int`, both `Float` or both `BigInt` | the same type |
| `++` | both `String` or both `List a` | the same type |
| `< <= > >=` | both `Int`, `Float`, `BigInt` or `String` | `Bool` |
| `== !=` | the same type | `Bool` |
| `&& \|\|`, `not` | `Bool` | `Bool` |
| `x \|> f` | `f x` | the result of `f` |

`1.5 + 1n` and `"a" ++ 1` are `FLICK-T003`. An operand nothing else
determines is `Int` (`String` for `++`). Ranges `a..b`, `a..<b` and `a..`
count in `Int`.

One function serves every numeric type through the closed classes of the
module `Numeric`: `Numeric a` (`Int`, `Float`, `BigInt`), `Integer a` (`Int`,
`BigInt`) and `Floating a` (`Float`). On a type variable with one of them,
the operators and integer literals use its methods (`add`, `div`,
`from_int`, …); a literal with a fraction needs `Floating`. The module has
every instance, and a program cannot declare another (`FLICK-C012`).

```flick
import Numeric exposing (Floating, Numeric)

square : Numeric a => a -> a
let square x = x * x

mean : Floating a => List a -> a
let mean xs = List.sum xs / Numeric.from_int (List.length xs)
```

`List.sum` and `List.product` take any `Numeric`. Without the constraint,
`x + 1` on a type variable is `FLICK-C001`. `< <= > >=` on a type variable
with a numeric class use its `lt`, `le`, `gt`, `ge`, which mean what the
operators mean on its type (NaN is unordered); with only `Ord a`, they
compare by `compare`.

Counts, sizes, indices, hashes and calendar parts are `Int` throughout the
standard library (`List.length`, `HashMap.size`, `Hash.hash`, `Queue.size`,
`Schedule.recurs`, the parts of a `DateTime`); milliseconds, epochs, byte
sizes and measurements stay `Float`, as 32 bits do not hold them.

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
that call is `FLICK-T015`. The members of a `rec` group call each other at
their own types without one, whatever their variables are named: `odd_len`
of `List b -> Bool` called from `even_len` of `List a -> Bool` on a
`List a` is the same type.

```flick
type Nested a = Flat a | Nest (Nested (List a))

depth : forall a. Nested a -> Float
let rec depth value =
  match value with
  | Flat _ -> 0
  | Nest inner -> 1 + depth inner
```

## Generalization

A binding is polymorphic only when it is built purely: no runtime call runs
while it is defined (see [Generalization in the presence of Ref](../../spec/05-types-and-classes/#generalization-in-the-presence-of-ref)). Lambdas and `task`
bodies are deferred and count as pure. A polymorphic signature, annotation or
higher-rank argument over a runtime result is `FLICK-T013`, and so is a
top-level value whose type stays unknown. Type variables of an enclosing
signature, or of a value already in scope, are never generalized, and mutable
containers are invariant. A row of calls still unknown when a binding is
generalized becomes `none`, as at the end of a declaration
([effects](../effects/)); it is not quantified.

```flick
make_empty : forall a. Unit ->{} Ref (Option a)
let make_empty () =
  Ref.make None

fresh_cell : forall a. Eff none none (Ref (Option a))
let fresh_cell =
  task { Ref.make None }
```

Classes and instances: [classes.md](../classes/).

## Not checked yet

| Construct | Today |
| - | - |
| `sql`, `ui`, `codec`, `schema` blocks | any type |
