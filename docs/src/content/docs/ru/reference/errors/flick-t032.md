---
title: "FLICK-T032"
source_hash: "1a877c8fd7d30ae5"
---

Предупреждение: `Seq.to_list` из `for` строит ленивую последовательность только для того, чтобы сразу её форсировать.  
`[for … yield e]` выполняет одни и те же условия одновременно в цикле, без генератора. Быстрое исправление переписывает это; `flick check --fix` применяет его.

```flick ignore
evens : List Float -> List Float
let evens xs = [for x <- xs if x % 2 == 0 yield x]
```
