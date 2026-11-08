---
title: "FLICK-C013"
---

`deriving` covers `Eq`, `Ord`, `Hash` and `Show` of nominal types. Every
field needs an instance of the class, and `Hash` and `Ord` are derived
together with `Eq`, so all of them use the same fields. A field like a `Ref`
has no `Hash`: write the instance by hand or wrap the field.

```flick ignore
type Point = Point Float Float
  deriving Eq, Hash, Show
```
