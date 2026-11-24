---
title: "FLICK-T089"
---

`Int` is a 32-bit signed integer (`-2147483648..=2147483647`) whose
arithmetic wraps at runtime. To keep reasoning sound in mathematical
integers, `flick prove` checks that every `+`, `-`, and unary `-` evaluated
in a contracted function's body stays within `-2147483648..=2147483647`
under the path's conditions. Add a `require` or a branch guard that rules
out overflow.

```flick fail T089
next : Int -> Int
let next x
where
  ensure (fun y -> y > x)
=
  x + 1
```

is

```flick
next : Int -> Int
let next x
where
  require (x < 2147483647)
  ensure (fun y -> y > x)
=
  x + 1
```
