---
title: "FLICK-T034"
---

A warning: a `match` on a `Result` whose `Ok x` arm gives `x` and whose
`Err e` arm fails with `e` (`from_result (Err e)`) is `r?`. The quick fix
writes `?`; `flick check --fix` applies it.

```flick fail T034
# type error Invalid = Invalid String
# parse : String -> Result Invalid Float
# let parse s = Ok 0.0
total : String ->{; Invalid} Float
let total s =
  match parse s with
  | Ok n -> n
  | Err e -> from_result (Err e)
```

is

```flick
# type error Invalid = Invalid String
# parse : String -> Result Invalid Float
# let parse s = Ok 0.0
total : String ->{; Invalid} Float
let total s = (parse s)?
```
