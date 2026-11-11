---
title: "FLICK-S016"
source_hash: "719a529495650f8e"
---

`let mut` объявляет переменную одной функции: имя, которое связывает оператор блока её тела. Определение модуля, функция, шаблон, `let assert` и привязки `do` и `for` не могут быть `mut`. Свяжите шаблон через `let`, а изменяемую часть — через `let mut` одного имени; чтобы менять функцию, присваивайте переменной `fun`.

```flick
module App.Totals exposing (largest)

largest : (Int, Int) -> Int
let largest pair =
  let (a, b) = pair
  let mut best = a
  if b > best then best = b
  best
```
