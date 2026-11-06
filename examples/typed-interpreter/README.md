# A typed interpreter built from GADTs

An interpreter usually checks types twice: once when it reads a program, and
again at run time, when `eval` takes an expression apart and must handle the
"impossible" case of adding a number to a boolean. A GADT keeps the type of
an expression in its type, `Expr a` for an expression that evaluates to an
`a`, so `eval` returns an `a` and has no such case. This example builds that
interpreter in Flick: the typed expressions, `eval` and a pretty-printer, a
check that turns untyped terms into typed ones with `TypeEq`, and a value
that carries its own `Show`.

## Run it from the repository root

Use the checkout's [source setup](../../README.md#try-it-from-source), including
the pinned Rust toolchain, Node.js and pnpm:

```sh
pnpm install --frozen-lockfile
cargo build -p flick

./target/debug/flick run --project examples/typed-interpreter
./target/debug/flick test --project examples/typed-interpreter
./target/debug/flick test --project examples/typed-interpreter --emit go
```

If `flick` is already on your `PATH`, replace `./target/debug/flick` with `flick`.
The program checks five terms and prints the value of each well-typed one,
or why it has no type:

```text
3
10
true
type error: expected Int, found Bool
type error: expected Bool, found Int
```

## Expressions that know their type

Each constructor names the type it builds after `:`. `IntLit` builds an
`Expr Int`, `Less` an expression of a function from two `Int`s to a `Bool`.
`Apply` takes a function of some type `b -> a` and an argument of that `b`;
`b` is not in its result, so an `Apply` hides it. `Named` is an ordinary
constructor: it builds `Expr a` for any `a`.

```flick
type Expr a =
  | IntLit Int : Expr Int
  | BoolLit Bool : Expr Bool
  | Plus : Expr (Int -> Int -> Int)
  | Less : Expr (Int -> Int -> Bool)
  | If (Expr Bool) (Expr a) (Expr a) : Expr a
  | Apply (Expr (b -> a)) (Expr b) : Expr a
  | Named String (Expr a)

eval : forall a. Expr a -> a
let rec eval e =
  match e with
  | IntLit n -> n
  | BoolLit b -> b
  | Plus -> fun x y -> x + y
  | Less -> fun x y -> x < y
  | If c t f -> if eval c then eval t else eval f
  | Apply f x -> (eval f) (eval x)
  | Named _ inner -> eval inner
```

In the `IntLit` branch the checker knows that `a` is `Int`, so `n` is the
result; in the `Plus` branch `a` is `Int -> Int -> Int`, so the function is.
An editor shows `x : Int (= a)` where a value of type `a` is an `Int`. The
branches need a complete signature: `eval : forall a. Expr a -> a` is what
lets each of them learn something different about `a`.

A `match` on an `Expr Int` needs no arm for `BoolLit`, `Less`: no value of
that type is built by them, and an arm for one would never run.

## From untyped terms to typed expressions

A parser builds untyped terms; nothing in `TmPlus` says that its operands
must be numbers. The check finds out, and keeps the type it found as a value:
`Ty a` describes `a`. Comparing two descriptions gives `TypeEq a b`, the
evidence that two types are one. A branch on `Refl` turns an `Expr a` into
the `Expr b` the caller wants.

```flick
import TypeEq exposing (TypeEq(..))

type Ty a =
  | TInt : Ty Int
  | TBool : Ty Bool

equal : forall a b. Ty a -> Ty b -> Option (TypeEq a b)
let equal a b =
  match (a, b) with
  | (TInt, TInt) -> Some Refl
  | (TBool, TBool) -> Some Refl
  | _ -> None

coerce : forall a b. Ty a -> Ty b -> a -> Result String b
let coerce got want x =
  match equal got want with
  | Some Refl -> Ok x
  | None -> Err "the types differ"
```

`Interp.Check` does this for each operand. A typed expression whose type is
known only at run time is a `Typed`, which hides the type next to its
description: `| Typed (Ty a) (Expr a) : Typed`. A type error comes back as
the `Err` of a `Result`.

## A value with its own `Show`

The results have different types, but `show` needs one. A constructor can
hold the dictionary of a class with the value: `Showable x` is built where
the type of `x` is known and its `Show` is solved, and a branch on it shows
`x` with that `Show`.

```flick
type Showable =
  | Show a => Showable a : Showable

shown : Showable -> String
let shown s =
  match s with
  | Showable x -> show x

lines : List String
let lines = List.map shown [Showable 1, Showable true, Showable [2, 3]]
```

Only a `match` arm takes the dictionary out: the pattern of a `let` or of a
parameter cannot, since what it learns would have no end.

## What is where

- `src/Interp/Expr.fl`: `Expr a`, `eval` and `pretty`.
- `src/Interp/Ty.fl`: `Ty a`, its description and `equal`.
- `src/Interp/Check.fl`: untyped `Term`s, `Typed` and the check.
- `src/Interp/Value.fl`: `Showable` and running a term.
- `test/Interp/InterpTest.fl`: the tests, on Node and on Go.

The [types reference](../../docs/src/content/docs/reference/types.md#gadts)
describes GADTs, and the [classes reference](../../docs/src/content/docs/reference/classes.md#dictionaries-in-values)
dictionaries in values.
