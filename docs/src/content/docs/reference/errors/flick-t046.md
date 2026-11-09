---
title: "FLICK-T046"
---

A `match` with the arms `true` and `false` (or `_`) is an `if`. The quick
fix rewrites it when each arm fits on a line.

```flick
label : Bool -> String
let label done = if done then "done" else "pending"
```
