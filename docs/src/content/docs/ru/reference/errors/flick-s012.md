---
title: "FLICK-S012"
source_hash: "7a450f63eab7465f"
---

Шаблон связывает свои имена непоследовательно. Каждая альтернатива or-шаблона `p | q` должна связывать ровно одинаковые имена (проверяющий также требует одинаковые типы), чтобы тело ветви могло использовать их независимо от того, какая альтернатива совпала. Имя может быть связано только один раз одним шаблоном или параметрами одной функции.

```flick ignore
area : Shape -> Float
let area shape =
  match shape with
  | Square side | Circle side -> side * side
  | Rect width height when width == height -> width * width
  | Rect width height -> width * height
```
