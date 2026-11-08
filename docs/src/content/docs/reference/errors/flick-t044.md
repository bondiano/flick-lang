---
title: "FLICK-T044"
---

A value compared with itself gives the same answer every time: `x == x`,
`x <= x` and `x >= x` are `true`, the others `false`. It is usually a typo
for another name.

```flick ignore
same : Point -> Point -> Bool
let same a b = a.x == b.x
```
