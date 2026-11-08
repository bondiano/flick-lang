---
title: "FLICK-T002"
---

A type is used where a type of another kind is expected. `Eff` takes a service
row, an error row and a type: `Eff Unit Unit a` is rejected because `Unit` is a
`Type`, not a row. Service rows, error rows and capture rows are different kinds
and do not mix; an error row holds only types declared with `error`; a type
constructor takes as many arguments as its kind allows.

```flick ignore
load : UserId -> Eff {Users} {NotFound} User   // not `Eff Unit Unit User`
```
