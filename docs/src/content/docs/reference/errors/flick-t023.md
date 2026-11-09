---
title: "FLICK-T023"
---

`echo e` prints `e` with the module and line it is on, and returns it
unchanged; `x |> echo` does the same in a pipeline. It is for debugging and
does not show up in the function's effects, so the warning keeps it from
being forgotten.

```flick fail T023
module App.Totals exposing (total)

total : List Float -> Float
let total xs = echo (Float.sum_all xs)
```

The warning's fix removes `echo`. `flick pack` refuses a package with
`echo` left in it.
