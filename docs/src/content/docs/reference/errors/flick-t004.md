---
title: "FLICK-T004"
---

Every top-level function has its full signature right above its `let`, also a
function obtained by partial application; a recursive local function has one
too. Another local function may omit it, and an instance method takes the one of
its class. The signature states every argument type, the result and the effect
of each arrow; the body is checked against it.

```flick
greet : String -> String -> String
let greet prefix name =
  "\{prefix}, \{name}"

hello : String -> String
let hello = greet "Hello"
```
