---
title: "FLICK-T030"
---

A warning: `x <- e` followed by `pure x` at the end of a `do` block is `e`
itself, by the laws of `Monad`. The quick fix keeps `e`; `flick check --fix`
applies it.

```flick ignore
first_user : Option User -> Option User
let first_user found = do { user <- found; pure user }
```
