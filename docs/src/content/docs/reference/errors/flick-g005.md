---
title: "FLICK-G005"
---

A `let mut` variable belongs to the function that declares it: a `fun`, a
local function, a `task`, a lazy `for`, a `do` block or an arm of a handler
of operations inside it can run later, more than once or concurrently, so it
can neither read nor assign the variable. That keeps the function pure from
the outside. Bind the current value with `let` and use that name, or
return a new value from the inner function.

When the inner code must change the value, or see later changes, keep it in
a `Ref` instead: `let count = Ref.make 0`, `Ref.set count 1`,
`Ref.get count`. Each of these is a runtime call, so the functions around
them run at runtime too. An editor offers the rewrite as "keep `count` in a
`Ref`".

```flick
module App.Shift exposing (shifted)

shifted : List Int -> List Int
let shifted xs =
  let mut offset = 1
  offset = offset * 10
  let by = offset
  List.map (fun x -> x + by) xs
```
