---
title: "FLICK-T058"
---

Matching a constructor written with `: R` would teach the checker something
it does not learn:

- that two regions are the same region: a region is passed through a
  constructor as a variable and never refined;
- an equality of rows of services, errors or captures;
- an equality from a pattern that is not a branch of a `match`, such as the
  pattern of a `let` or of a parameter. Match it in a `match` instead.

```flick ignore
type Expr a =
  | IntLit Int : Expr Int
  | BoolLit Bool : Expr Bool

as_int : Expr Int -> Int
let as_int e =
  match e with
  | IntLit n -> n
```
