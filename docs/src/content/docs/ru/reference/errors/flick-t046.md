---
title: "FLICK-T046"
source_hash: "f2d405890073032a"
---

`match` с ветвями `true` и `false` (или `_`) является `if`. Быстрое исправление переписывает его, когда каждая ветвь помещается на строку.

```flick
label : Bool -> String
let label done = if done then "done" else "pending"
```
