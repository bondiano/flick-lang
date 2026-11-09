---
title: "FLICK-R012"
---

A module is imported, or a name is imported by `exposing`, and nothing in
the module uses it. The warning's fix removes the import or the name.

```flick ignore
module App.Report exposing (total)

import List exposing (map, filter)

total : List Float -> Float
let total xs = List.sum (map (fun x -> x * 2) xs)
```

`filter` is never used. An import whose module declares instances is kept:
importing it can matter for its instances alone.
