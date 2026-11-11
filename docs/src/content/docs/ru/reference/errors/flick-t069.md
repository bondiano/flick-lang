---
title: "FLICK-T069"
source_hash: "6240c8c0a8594778"
---

`name = value` присваивает переменной `let mut` той функции, в которой стоит. Имя, связанное обычным `let`, параметр, переменная шаблона, определение модуля или импорт присвоить нельзя. Объявите переменную через `let mut` (исправление обычного `let` добавляет `mut`) или свяжите новое имя через `let`.

```flick
module App.Count exposing (next)

next : Int -> Int
let next n =
  let mut count = n
  count = count + 1
  count
```
