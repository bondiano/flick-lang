---
title: "FLICK-T014"
source_hash: "ebd1d8516544c769"
---

Метка появляется дважды в одном типе записи, литерале записи, обновлении или шаблоне.
Метки записи уникальны; их порядок не имеет значения.

```flick ignore
type alias Point = { x : Float; y : Float }

origin : Point
let origin = { x = 0; y = 0 }
```
