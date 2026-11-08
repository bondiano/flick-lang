---
title: "FLICK-T050"
source_hash: "2e59bf6c580ea93b"
---

`List.concat (List.map f xs)` строит список списков, а затем объединяет его.  
`List.flat_map f xs` выполняет обе операции за один проход. Быстрое исправление переписывает это.

```flick ignore
doubled : List Float -> List Float
let doubled xs = List.flat_map (fun x -> [x, x]) xs
```
