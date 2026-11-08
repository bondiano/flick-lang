---
title: "FLICK-T056"
---

A constructor written with `: R` builds `R`, and `R` must be its own type
applied to all of the type's parameters. An argument may be any type of kind
`Type`, a member of a `kind`, or a `Symbol`; an argument for a parameter that
is a region, a row or a capture set is passed on as a variable of the
constructor. A type that names its representation (`unboxed type`,
`untagged type`) has no such constructors.

```flick ignore
type Expr a =
  | IntLit Int : Expr Int
  | Add (Expr Int) (Expr Int) : Expr Int
  | If (Expr Bool) (Expr a) (Expr a) : Expr a
  | BoolLit Bool : Expr Bool
```

`| IntLit Int : Option Int` and `| IntLit Int : Expr` are reported.
