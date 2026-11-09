---
title: "FLICK-T061"
---

Inside the body of `try … with`, and in a loop (`for … do` or the clauses
and element of `[for …]`), a `let mut` variable declared outside cannot be
assigned yet: what it would hold after an error is caught or after the
loop is not settled. Assign the value of the whole `try`, or assign in one
of its arms; a variable declared inside the body may be assigned there.

```flick
module App.Parse exposing (Missing, port_of)

type error Missing = Missing String

lookup : String ->{; Missing} Int
let lookup key = from_result (Err (Missing key))

port_of : String ->{} Int
let port_of key =
  let mut port = 80
  port =
    try lookup key with
    | Missing _ -> port + 8000
  port
```
