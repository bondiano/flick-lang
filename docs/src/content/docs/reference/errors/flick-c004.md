---
title: "FLICK-C004"
---

With a functional dependency `c -> a`, the types at `c` determine those at
`a`: two instances whose determining types unify must agree on the rest.
Wrap one of the types to keep both relations.

```flick ignore
class Collection c a | c -> a =
  empty : c

instance Collection UserSet User =
  empty : UserSet
  let empty = UserSet []
```
