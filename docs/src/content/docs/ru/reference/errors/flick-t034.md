---
title: "FLICK-T034"
source_hash: "81e74435eb722a62"
---

Предупреждение: `match` на `Result`, у которого `Ok x` ветвь возвращает `x`, а `Err e` ветвь завершается с ошибкой `e` (`Fail.fail e`), является `r?`. Быстрое исправление записывает `?`; `flick check --fix` применяет его.

```flick fail T034
# type error Invalid = Invalid String
# parse : String -> Result Invalid Float
# let parse s = Ok 0.0
total : String ->{; Invalid} Float
let total s =
  match parse s with
  | Ok n -> n
  | Err e -> Fail.fail e
```

это

```flick
# type error Invalid = Invalid String
# parse : String -> Result Invalid Float
# let parse s = Ok 0.0
total : String ->{; Invalid} Float
let total s = (parse s)?
```
