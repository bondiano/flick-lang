---
title: "FLICK-R020"
source_hash: "ffa3bc047028d77f"
---

Переменная `let mut`, которой не присваивает ни один оператор, никогда не меняется, поэтому `mut` ей не нужен: читатель кода стал бы искать присваивание. Исправление убирает `mut`.

```flick
module App.Area exposing (area)

area : Float -> Float -> Float
let area width height =
  let size = width * height
  size
```
