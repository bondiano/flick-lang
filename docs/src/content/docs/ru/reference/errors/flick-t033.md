---
title: "FLICK-T033"
source_hash: "bb3a390e55aa6d41"
---

Предупреждение: `List.filter (fun x -> p)`, переданный в `List.map (fun x -> e)`, проходит по списку дважды и строит промежуточный список. `[for x <- xs if p yield e]` выполняет обе операции в одном цикле. Быстрое исправление переписывает код, если обе функции используют одинаковое имя параметра.

```flick
doubled_evens : List Float -> List Float
let doubled_evens xs = [for x <- xs if x % 2 == 0 yield x * 2]
```
