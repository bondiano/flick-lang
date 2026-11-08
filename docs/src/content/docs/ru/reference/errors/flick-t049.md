---
title: "FLICK-T049"
source_hash: "fc6e2a4eb124960f"
---

`List.length xs == 0` подсчитывает весь список, чтобы узнать, есть ли в нем элемент. `List.is_empty xs` утверждает это, а `not (List.is_empty xs)` — наоборот. Быстрое исправление переписывает `== 0`, `!= 0`, `> 0`, `>= 1` и `< 1`.

```flick ignore
nothing : List Float -> Bool
let nothing xs = List.is_empty xs
```
