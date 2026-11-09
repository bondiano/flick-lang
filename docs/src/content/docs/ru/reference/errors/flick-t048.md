---
title: "FLICK-T048"
source_hash: "c7ac6cba5ec3445d"
---

Длина `List.filter` по сравнению с нулём строит список для подсчёта.
`List.any p xs` останавливается при первом совпадении; `not (List.any p xs)` сообщает,
что совпадений нет. Быстрое исправление переписывает это.

```flick
has_adult : List Float -> Bool
let has_adult ages = List.any (fun a -> a >= 18) ages
```
