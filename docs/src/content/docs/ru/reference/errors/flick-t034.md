---
title: "FLICK-T034"
source_hash: "e4fe6b1ba42dc9de"
---

Предупреждение: `match` на `Result`, у которого `Ok x` ветвь возвращает `x`, а `Err e` ветвь завершается с ошибкой `e` (`from_result (Err e)`), является `r?`. Быстрое исправление записывает `?`; `flick check --fix` применяет его.

```flick ignore
total : String ->{; Invalid} Float
let total s =
  match parse s with
  | Ok n -> n
  | Err e -> from_result (Err e)
```

это

```flick ignore
total : String ->{; Invalid} Float
let total s = (parse s)?
```
