---
title: "FLICK-T007"
---

A type variable bound by a `forall` is used outside that `forall` (skolem
escape). A callback of type `forall a. a -> Unit` must work for every `a`, so it
cannot store its argument into a cell created outside it, or return something
whose type mentions that `a`.

```flick ignore
log_all : (forall a. a ->{} Unit) ->{} Unit
let log_all log =
  log 1
  log "text"
```
