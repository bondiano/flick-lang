---
title: "FLICK-T018"
source_hash: "41340614ba4dee91"
---

Паттерн конструктора содержит другое количество аргументов, чем принимает конструктор. `Circle Point Float` принимает два аргумента; поле записи — это один аргумент.

```flick ignore
type Shape = Circle Float Float | Empty

radius : Shape -> Float
let radius shape =
  match shape with
  | Circle _ r -> r
  | Empty -> 0
```
