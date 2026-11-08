---
title: "FLICK-T048"
---

The length of a `List.filter` compared with zero builds a list to count
it. `List.any p xs` stops at the first match; `not (List.any p xs)` says
that none matches. The quick fix rewrites it.

```flick ignore
has_adult : List Float -> Bool
let has_adult ages = List.any (fun a -> a >= 18) ages
```
