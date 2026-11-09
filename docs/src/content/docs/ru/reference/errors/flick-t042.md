---
title: "FLICK-T042"
source_hash: "d29723f4f1f27235"
---

`else`, единственным выражением которого является другой `if`, создаёт вложенность уровня без необходимости: `else if` продолжает цепочку. Быстрое исправление объединяет их.

```flick
sign : Float -> String
let sign n = if n > 0 then "+" else if n < 0 then "-" else "0"
```
