---
title: "FLICK-T033"
---

A warning: `List.filter (fun x -> p)` piped into `List.map (fun x -> e)`
walks the list twice and builds a list in between. `[for x <- xs if p yield e]`
does both in one loop. The quick fix rewrites it when both functions name
their parameter the same.

```flick ignore
doubled_evens : List Float -> List Float
let doubled_evens xs = [for x <- xs if x % 2 == 0 yield x * 2]
```
