---
title: "FLICK-T047"
---

`List.head (List.filter p xs)` tests every element and builds a list only
to take its first. `List.find p xs` stops at the first match. The quick
fix rewrites it, written either way or through `|>`.

```flick
first_adult : List Float -> Option Float
let first_adult ages = List.find (fun a -> a >= 18) ages
```
