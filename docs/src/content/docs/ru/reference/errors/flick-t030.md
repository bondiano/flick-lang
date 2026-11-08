---
title: "FLICK-T030"
source_hash: "9d868a869ec50e7f"
---

Предупреждение: `x <- e`, за которым следует `pure x` в конце блока `do`, само по себе является `e` согласно законам `Monad`. Быстрое исправление сохраняет `e`; `flick check --fix` применяет его.

```flick ignore
first_user : Option User -> Option User
let first_user found = do { user <- found; pure user }
```
