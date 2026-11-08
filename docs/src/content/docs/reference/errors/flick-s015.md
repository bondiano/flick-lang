---
title: "FLICK-S015"
---

A constructor names the type it builds after `:` (`| IntLit Int : Expr Int`)
only in a `type` declaration, and only when it has no literal value. The
constructor of a `type error` and a member of a `kind` build their own type,
and the constructor of an `untagged type` is its literal.

```flick ignore
type Expr a =
  | IntLit Int : Expr Int
  | BoolLit Bool : Expr Bool
```

`type error NotFound = NotFound String : NotFound` and
`| Get = "GET" : Method` are reported.
