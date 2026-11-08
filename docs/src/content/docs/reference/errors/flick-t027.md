---
title: "FLICK-T027"
---

The project sets `no-let-assert = true` in the `[lint]` table of
`flick.toml`: no `let assert` may stop its programs. Handle every value
with `match`; the fix moves the rest of the block into the arm of the
pattern and adds an arm `-> todo` for what it misses.

```flick ignore
module App.Config exposing (port)

port : List Float -> Float
let port ports =
  match ports with
  | [first, ..._] -> first
  | [] -> 8080
```
