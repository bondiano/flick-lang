---
title: "FLICK-T047"
source_hash: "ed65f3f87d0052a8"
---

`List.head (List.filter p xs)` проверяет каждый элемент и строит список только для того,
чтобы взять его первый элемент. `List.find p xs` останавливается при первом совпадении.
Быстрое исправление переписывает это, написанное в любом виде или через `|>`.

```flick ignore
first_adult : List Float -> Option Float
let first_adult ages = List.find (fun a -> a >= 18) ages
```
