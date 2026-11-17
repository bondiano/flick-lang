---
title: "FLICK-T075"
---

A warning: `from_result (Err e)` builds a `Result` only to fail with its
error. `Fail.fail e` fails with `e` directly, with the same type,
`Errors e ->{; e} a`, in any function whose row allows `e`. The quick fix
writes it; `flick check --fix` applies it.

```flick
# type error NotFound = NotFound Int
# find : Int -> Option String
# let find id = None
name : Int ->{; NotFound} String
let name id =
  match find id with
  | Some found -> found
  | None -> Fail.fail (NotFound id)
```
