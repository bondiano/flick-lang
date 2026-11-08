---
title: "FLICK-T050"
---

`List.concat (List.map f xs)` builds a list of lists and then joins it.
`List.flat_map f xs` does both in one pass. The quick fix rewrites it.

```flick ignore
doubled : List Float -> List Float
let doubled xs = List.flat_map (fun x -> [x, x]) xs
```
