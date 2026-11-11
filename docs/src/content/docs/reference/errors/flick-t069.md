---
title: "FLICK-T069"
---

`name = value` assigns a `let mut` variable of the function it is in. A
name bound by plain `let`, a parameter, a pattern variable, a definition of
the module or an import cannot be assigned. Declare the variable with
`let mut` (the fix of a plain `let` adds it), or bind a new name with
`let`.

```flick
module App.Count exposing (next)

next : Int -> Int
let next n =
  let mut count = n
  count = count + 1
  count
```
