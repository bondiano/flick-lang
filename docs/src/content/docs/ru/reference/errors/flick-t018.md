---
title: "FLICK-T018"
source_hash: "b6f466dd727ad21f"
---

Паттерн конструктора содержит другое количество аргументов, чем принимает конструктор. `Circle Point Float` принимает два аргумента; поле записи — это один аргумент.

```flick
type Shape = Circle Float Float | Empty

radius : Shape -> Float
let radius shape =
  match shape with
  | Circle _ r -> r
  | Empty -> 0
```
