---
title: "FLICK-T069"
---

`name = value` assigns a `let mut` variable of the function it is in. A
name bound by plain `let`, a parameter, a pattern variable, a definition of
the module or an import cannot be assigned. Declare the variable with
`let mut` (the fix adds it where that declaration is valid), or bind a new name with
`let`.

`let mut` is not allowed directly in a `do` block or a `for` clause. Those
declarations receive no automatic insertion; use an ordinary function or loop body.
The fix is also withheld when mutation would cross a function, task, handler-arm
or comprehension boundary.

```flick
module App.Count exposing (next)

next : Int -> Int
let next n =
  let mut count = n
  count = count + 1
  count
```
