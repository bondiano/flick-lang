---
title: "FLICK-T024"
---

A `_` in a signature or annotation leaves that part of the type to the
checker. The warning shows the type it inferred, and its fix writes it in
place of the `_`.

```flick fail T024
module App.Pairs exposing (swap)

swap : (Float, String) -> _
let swap (a, b) = (b, a)
```

The fix writes `(String, Float)`.

`todo` also leaves a type for the checker to infer, either as the whole
signature or inside one. Unlike `_`, it is an error until replaced. The editor
offers a replacement only when it can determine a complete type; for a GADT
match it checks the proposed signature before offering it.
