---
title: "FLICK-T010"
---

An arm can never run: the arms above it already match every value it matches.
Remove it, or move it above the arm that covers it.

```flick fail T010
sign : Float -> String
let sign n =
  match n with
  | _ -> "other"
  | 0 -> "zero"
```
