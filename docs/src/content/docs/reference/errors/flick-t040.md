---
title: "FLICK-T040"
---

An `if` whose branches are `true` and `false` returns its own condition,
or `not` of it. The quick fix writes the condition.

```flick
adult : Float -> Bool
let adult age = age >= 18
```
