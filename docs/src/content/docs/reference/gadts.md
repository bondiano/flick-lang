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

## Limits

- Only an arm of a `match` learns a type. The pattern of a `let`, of a parameter
  or an alternative of an or-pattern does not, and each is `FLICK-T058`.
- Patterns do not identify different regions (`FLICK-T058`).
  [Service, error and capture row indices](../row-indices/) can be refined.
- TypeScript has no refinement: in a `.d.ts` file each constructor is a case of
  every instance of its type, and a type it hides is `unknown`.
