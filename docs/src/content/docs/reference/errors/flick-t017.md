---
title: "FLICK-T017"
---

Recursion is explicit: a function that calls itself is defined with `let rec`,
and functions that call each other are defined together in one `rec { … }`
group, each with its own signature.

```flick
rec {
  is_even : Float -> Bool
  let is_even n = if n == 0 then true else is_odd (n - 1)

  is_odd : Float -> Bool
  let is_odd n = if n == 0 then false else is_even (n - 1)
}
```
