---
title: "FLICK-T044"
source_hash: "98eb248ad242b20a"
---

Значение, сравниваемое само с собой, даёт одинаковый результат каждый раз: `x == x`,  
`x <= x` и `x >= x` являются `true`, остальные — `false`. Обычно это опечатка  
для другого имени.

```flick ignore
same : Point -> Point -> Bool
let same a b = a.x == b.x
```
