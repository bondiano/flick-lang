---
title: "FLICK-R020"
---

A `let mut` variable that no statement assigns never changes, so it needs
no `mut`: a reader of the code would look for the assignment. The fix
removes `mut`.

```flick
module App.Area exposing (area)

area : Float -> Float -> Float
let area width height =
  let size = width * height
  size
```
