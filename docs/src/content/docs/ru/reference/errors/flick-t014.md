---
title: "FLICK-T014"
source_hash: "15500293a3f8b928"
---

Метка появляется дважды в одном типе записи, литерале записи, обновлении или шаблоне.
Метки записи уникальны; их порядок не имеет значения.

```flick
type alias Point = { x : Float; y : Float }

origin : Point
let origin = { x = 0; y = 0 }
```
