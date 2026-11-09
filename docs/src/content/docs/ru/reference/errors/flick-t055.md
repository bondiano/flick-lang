---
title: "FLICK-T055"
source_hash: "b8d1277e40f3854f"
---

Звено `_` в `match` для варианта типа этого модуля покрывает
конструкторы, не указанные в других звеньях, а также все конструкторы,
добавленные в тип позже: компилятор не может указать на это `match`, когда
такое происходит. Напишите звено для каждого конструктора или отключите предупреждение
для этого кода с помощью `[lint] allow`.

```flick
type Shape = Circle Float | Square Float | Dot

area : Shape -> Float
let area shape = match shape with
  | Circle r -> 3.14 * r * r
  | Square s -> s * s
  | Dot -> 0
```
