---
title: "FLICK-T030"
source_hash: "9d05f37b7f0170ff"
---

Предупреждение: `x <- e`, за которым следует `pure x` в конце блока `do`, само по себе является `e` согласно законам `Monad`. Быстрое исправление сохраняет `e`; `flick check --fix` применяет его.

```flick fail T030
# type User = User { name : String }
first_user : Option User -> Option User
let first_user found = do { user <- found; pure user }
```
