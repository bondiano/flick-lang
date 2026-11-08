---
title: "FLICK-E014"
---

An `abort` operation never returns: its handler's branch ends the handler.
So its result is a type variable that the operation quantifies and that
nothing else in its type names, and a call fits wherever a value is wanted.

```flick ignore
effect Throw =
  // was: abort throw : String ->{} Unit
  abort throw : forall b. String ->{} b
```
