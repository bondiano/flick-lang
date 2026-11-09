---
title: "FLICK-T055"
---

An `_` arm in a `match` on a variant type of this module covers the
constructors not written in the other arms, and also every constructor
added to the type later: the compiler cannot point at this `match` when
one is. Write an arm for each constructor, or leave the warning off for
this code with `[lint] allow`.

```flick ignore
type Shape = Circle Float | Square Float | Dot

area : Shape -> Float
let area shape = match shape with
  | Circle r -> 3.14 * r * r
  | Square s -> s * s
  | Dot -> 0
```
