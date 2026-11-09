---
title: "FLICK-T057"
---

A branch of a `match` on a constructor written with `: R` learns what the
type of the matched value is: in the `IntLit` branch of a match on an
`Expr a`, `a` is `Int`. The checker uses that only when the types are known:

- the function around the `match` has a complete signature (no `_` in it);
- the type of the matched value is known where it is matched, not inferred
  from the branches;
- inside the branch, nothing from outside it gets its type from what the
  branch learned. A type that is still unknown outside, such as the result
  of an unannotated `let`, needs an annotation.

```flick
type Expr a =
  | IntLit Int : Expr Int
  | BoolLit Bool : Expr Bool

eval : forall a. Expr a -> a
let eval e =
  match e with
  | IntLit n -> n
  | BoolLit b -> b
```

Without the signature of `eval`, the `match` is reported.
