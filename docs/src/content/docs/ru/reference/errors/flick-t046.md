---
title: "FLICK-T046"
source_hash: "226c02163f993836"
---

`match` с ветвями `true` и `false` (или `_`) является `if`. Быстрое исправление переписывает его, когда каждая ветвь помещается на строку.

```flick ignore
label : Bool -> String
let label done = if done then "done" else "pending"
```
