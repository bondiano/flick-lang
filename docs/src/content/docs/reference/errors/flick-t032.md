---
title: "FLICK-T032"
---

A warning: `Seq.to_list` of a `for` builds a lazy sequence only to force it.
`[for … yield e]` runs the same clauses at once in a loop, without a
generator. The quick fix rewrites it; `flick check --fix` applies it.

```flick ignore
evens : List Float -> List Float
let evens xs = [for x <- xs if x % 2 == 0 yield x]
```
