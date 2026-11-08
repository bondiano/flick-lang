---
title: "FLICK-T042"
source_hash: "854580b0420ad2aa"
---

`else`, единственным выражением которого является другой `if`, создаёт вложенность уровня без необходимости: `else if` продолжает цепочку. Быстрое исправление объединяет их.

```flick ignore
sign : Float -> String
let sign n = if n > 0 then "+" else if n < 0 then "-" else "0"
```
