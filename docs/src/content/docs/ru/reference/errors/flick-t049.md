---
title: "FLICK-T049"
source_hash: "4c473d0f0216321f"
---

`List.length xs == 0` подсчитывает весь список, чтобы узнать, есть ли в нем элемент. `List.is_empty xs` утверждает это, а `not (List.is_empty xs)` — наоборот. Быстрое исправление переписывает `== 0`, `!= 0`, `> 0`, `>= 1` и `< 1`.

```flick
nothing : List Float -> Bool
let nothing xs = List.is_empty xs
```
