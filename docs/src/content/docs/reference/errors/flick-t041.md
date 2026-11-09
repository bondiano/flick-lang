---
title: "FLICK-T041"
---

`if not c then a else b` makes the reader undo the `not`. With an `else`,
the same `if` without `not` and with the branches swapped says it
directly. The quick fix rewrites it when both branches fit on a line.

```flick
label : Bool -> String
let label done = if done then "done" else "pending"
```
