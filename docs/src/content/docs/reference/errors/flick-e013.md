---
title: "FLICK-E013"
---

A service is an effect whose operations are all `tail`: a handler gives each
operation its value, and a record of functions, `S.Impl`, is such a handler.
An operation that never returns (`abort`) or holds the rest of the
computation (`control`) belongs in an `effect`.

```flick ignore
effect Throw =
  // was, in `service Throw =`: abort throw : forall b. String ->{} b
  abort throw : forall b. String ->{} b
```
