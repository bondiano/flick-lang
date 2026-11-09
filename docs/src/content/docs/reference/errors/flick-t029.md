---
title: "FLICK-T029"
---

A warning: a `do` block over `Eff` builds a computation through
`Monad (Eff r e)`, which fixes the rows of every statement to one. A
`task { … }` with direct calls builds the same cold computation and joins the
rows of its calls. The quick fix rewrites `x <- e` as `let x = await e`, a
statement as `await e`, and a final `pure v` as `v`.

```flick
# service Users =
#   get : Int ->{; NotFound} User
#   touch : UserId ->{} Unit
# type UserId = UserId String
# type User = User { name : String }
# type error NotFound = NotFound String
load_both : Eff {Users} {NotFound} (User, User)
let load_both =
  task {
    let first = await (task { Users.get 1 })
    let second = await (task { Users.get 2 })
    (first, second)
  }
```
