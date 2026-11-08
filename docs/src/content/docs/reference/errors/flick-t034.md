---
title: "FLICK-T034"
---

A warning: a `match` on a `Result` whose `Ok x` arm gives `x` and whose
`Err e` arm fails with `e` (`from_result (Err e)`) is `r?`. The quick fix
writes `?`; `flick check --fix` applies it.

```flick ignore
total : String ->{; Invalid} Float
let total s =
  match parse s with
  | Ok n -> n
  | Err e -> from_result (Err e)
```

is

```flick ignore
total : String ->{; Invalid} Float
let total s = (parse s)?
```
