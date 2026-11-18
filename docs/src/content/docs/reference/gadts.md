---
title: "GADTs"
---

In a generalized algebraic data type each constructor says which type it
builds. A `match` then learns that type in a branch: an evaluator returns
exactly what its input describes, and an expression that makes no sense does
not compile. Everything on this page runs on JavaScript and Go. The rules in
full are in [Kinds and types](../types/#gadts).

## Declaring one

After `:`, a constructor names the type it builds:

```flick
type Expr a =
  | NatLit Int : Expr Int
  | BoolLit Bool : Expr Bool
  | Add (Expr Int) (Expr Int) : Expr Int
  | If (Expr Bool) (Expr a) (Expr a) : Expr a
```

`NatLit` builds an `Expr Int` and `BoolLit` an `Expr Bool`, while `If` builds
the type of its two branches. Every use is held to these types, so adding a
boolean to a number is `FLICK-T003` before anything runs:

```flick fail T003
# type Expr a =
#   | NatLit Int : Expr Int
#   | BoolLit Bool : Expr Bool
#   | Add (Expr Int) (Expr Int) : Expr Int
#   | If (Expr Bool) (Expr a) (Expr a) : Expr a
bad : Expr Int
let bad = Add (NatLit 1) (BoolLit true)
```

## A branch learns the type

Matching a constructor tells the branch what `a` is, so one function returns a
different type for each input:

```flick
# type Expr a =
#   | NatLit Int : Expr Int
#   | BoolLit Bool : Expr Bool
#   | Add (Expr Int) (Expr Int) : Expr Int
#   | If (Expr Bool) (Expr a) (Expr a) : Expr a
eval : forall a. Expr a -> a
let rec eval e =
  match e with
  | NatLit n -> n
  | BoolLit b -> b
  | Add x y -> eval x + eval y
  | If c t f -> if eval c then eval t else eval f
```

In the `NatLit` branch `a` is `Int`, so `n` is a result of type `a`. In the
`BoolLit` branch `a` is `Bool`. An editor shows such a value as `Int (= a)`.
`eval (If (BoolLit true) (Add (NatLit 1) (NatLit 2)) (NatLit 0))` is `3`.

## Where a signature is needed

A branch learns only under a complete signature. A top-level function always
has one, and without it `eval` is `FLICK-T004`. That also holds when no branch
relies on what it could learn:

```flick
# type Expr a =
#   | NatLit Int : Expr Int
#   | BoolLit Bool : Expr Bool
#   | Add (Expr Int) (Expr Int) : Expr Int
#   | If (Expr Bool) (Expr a) (Expr a) : Expr a
is_literal : Expr a -> Bool
let is_literal e =
  match e with
  | NatLit _ -> true
  | BoolLit _ -> true
  | Add _ _ -> false
  | If _ _ _ -> false
```

Other local functions need no signature, but one that matches a value of a GADT
does: without it the type of the value is not known in the `match`, which is
`FLICK-T057`.

## A type with structure

What a branch learns may be a type made of parts:

```flick
type Two a =
  | One Int : Two Int
  | Both (Two x) (Two y) : Two (x, y)

eval_two : forall a. Two a -> a
let rec eval_two t =
  match t with
  | One n -> n
  | Both l r -> (eval_two l, eval_two r)
```

In the `Both` branch `a` is the pair `(x, y)`, so a pair is what the branch
returns. `eval_two (Both (One 1) (One 2))` is `(1, 2)`.

## No arm for what cannot be

A function that takes an `Expr Int` needs no `BoolLit` arm:

```flick
# type Expr a =
#   | NatLit Int : Expr Int
#   | BoolLit Bool : Expr Bool
#   | Add (Expr Int) (Expr Int) : Expr Int
#   | If (Expr Bool) (Expr a) (Expr a) : Expr a
is_nat_literal : Expr Int -> Bool
let is_nat_literal e =
  match e with
  | NatLit _ -> true
  | Add _ _ -> false
  | If _ _ _ -> false
```

An arm for `BoolLit` here is the warning `FLICK-T010`, since it is never
reached. The `If` arm is required, because an `If` can build an `Expr Int`, and
without it the `match` is `FLICK-T009`.

## Values that share a type

The check follows a type variable across the parts of the matched value:

```flick
type Tagged a =
  | IsNat : Tagged Int
  | IsBool : Tagged Bool

agree : Tagged a -> Tagged a -> Bool
let agree x y =
  match (x, y) with
  | (IsNat, IsNat) -> true
  | (IsBool, IsBool) -> true
```

Once `x` is `IsNat`, `a` is `Int` and `y` cannot be `IsBool`, so a mixed pair is
never built: it needs no arm, and an arm for it is `FLICK-T010`. With
`Tagged a -> Tagged b` the two types may differ, and the same `match` is
`FLICK-T009`. A type the constructor hides is not followed: a `match` on
`| Packed (Tagged x) (Tagged x) : Packed` needs a `Packed _ _` arm for the
mixed values, although none is built.

## Evidence that two types are one

A constructor may require two types to be equal:

```flick
type Equ a b =
  | Refl : Equ a a

coerce : Equ a b -> a -> b
let coerce eq =
  match eq with
  | Refl -> fun x -> x
```

Matching `Refl` tells the branch that `a` is `b`, so the input is already the
result and nothing happens at run time. You need not declare this type: the
core module `TypeEq` has it, as `TypeEq a b`, `Refl` and
[`TypeEq.cast`](../types/#gadts).

## Type equality

Row expressions are compared by their meaning throughout a type, including
inside invariant containers. Reordering services, repeating the same service
or grouping unions differently preserves the type.

```flick
service Log =
  line : String ->{} Unit

service Clock =
  now : Unit ->{} Int

same : Ref (Eff {Log, Clock} none Unit) -> Ref (Eff {Clock, Log} none Unit)
let same cell = cell
```

The reference still has one content type. Equality does not allow widening
the requirements or errors of a computation stored in it:

```flick fail T003
# service Log =
#   line : String ->{} Unit
# service Clock =
#   now : Unit ->{} Int
widen : Ref (Eff {Log} none Unit) -> Ref (Eff {Log, Clock} none Unit)
let widen cell = cell
```

The same distinction applies to nominal arguments and record fields. Type equality preserves service identities and their type
arguments, pure/runtime stages, regions and foreign contracts. It does not
make different nominal declarations interchangeable. See [kinds and types](../types/) and
[effects](../effects/).

A GADT branch's unification problem also uses type equality: a different
presentation of the same row cannot make a branch impossible. Patterns can
also [refine service, error and capture row indices](#row-indices).
Region identities remain rigid.

These are shared language rules for JS and Go. A backend receives the checked
program; its output representation does not choose a different type equality.

## Row indices

A constructor result can fix or relate indices of kind `ServiceRow`,
`ErrorRow` or `CaptureRow`. A `match` branch checks its body under those
equations. The function still needs a complete signature.

```flick
service Db =
  get : Int ->{} Int

type Cmd (r : ServiceRow) a =
  | Value a : Cmd none a
  | Read Int : Cmd {Db} Int

run : Cmd r a ->{r} a
let run command =
  match command with
  | Value value -> value
  | Read key -> Db.get key
```

The equation in the `Read` branch permits `Db`. It preserves the runtime
stage: even a call whose requirements become empty remains a runtime call.
Refinement cannot make the same implementation a pure function.

```flick fail E001
# service Db =
#   get : Int ->{} Int
# type Cmd (r : ServiceRow) =
#   | Read Int : Cmd {Db}
pure : Cmd r -> Int
let pure command =
  match command with
  | Read key -> Db.get key
```

An equation can have several residuals. In this example `s` may be empty
or contain `Db`; the branch proves that every allowed `s` fits `{Db}`.
It does not choose an empty residual.

```flick
# service Db =
#   get : Int ->{} Int
type Packed (r : ServiceRow) =
  | Packed (Eff s none Int) : Packed {Db | s}

unpack : Packed {Db} ->{Db} Int
let unpack packed =
  match packed with
  | Packed work -> await work
```

Exhaustiveness keeps these equations while checking the other parts of a
tuple. Here the first part proves that `r` contains `Db`, so `Empty` cannot
occur in the second part:

```flick
# service Db =
#   get : Int ->{} Int
type Includes (r : ServiceRow) (s : ServiceRow) =
  | Includes : Includes r {Db | r}

type RowCmd (r : ServiceRow) =
  | Empty : RowCmd none
  | WithDb Int : RowCmd {Db}

read_value : (Includes r r, RowCmd r) -> Int
let read_value pair =
  match pair with
  | (Includes, WithDb value) -> value
```

Hidden residuals of separate constructor occurrences remain independent.
Only their shared outer indices relate the parts of the value.

The same reasoning applies to error rows. Local handlers remove the
requirements they cover before the surrounding signature is checked.
Equations stay inside the branch, including checks deferred until the
declaration ends. Service and error arguments remain invariant: knowing
that a row contains `Store String` never permits `Store Int`.

Capture indices can relate the permitted captures of values:

```flick
type Share (c : CaptureRow) (d : CaptureRow) =
  | Share : Share c c

share : Share c d -> (Unit -> Int)^c -> (Unit -> Int)^d
let share proof callback =
  match proof with
  | Share -> callback
```

This does not identify distinct regions. Parameters of kind `Region` and
record-row parameters must still be passed as variables, and patterns do
not learn a new region identity. Refinement also preserves the existing
rules for handlers, foreign calls and mutable references.

These rules are shared by JS and Go. [Type equality](#type-equality)
and the [limits](#limits) below are the other constraints on a refining branch.

## Limits

- Only an arm of a `match` learns a type. The pattern of a `let`, of a parameter
  or an alternative of an or-pattern does not, and each is `FLICK-T058`.
- Patterns do not identify different regions (`FLICK-T058`).
  [Service, error and capture row indices](#row-indices) can be refined.
- TypeScript has no refinement: in a `.d.ts` file each constructor is a case of
  every instance of its type, and a type it hides is `unknown`.
