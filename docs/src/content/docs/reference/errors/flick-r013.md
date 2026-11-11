---
title: "FLICK-R013"
---

A parameter, pattern variable or local `let` is bound and never used. Use
it, or start its name with `_` to say it is unused on purpose; the
warning's fix does that.

A mutable local that is assigned but never read also gets this warning. Rename
its assignment targets too; a binder-only rename fix is withheld for that case.

```flick fail R013
module App.Pairs exposing (first)

first : (a, b) -> a
let first (x, y) = x
```

Write `(x, _y)` or `(x, _)`. A field of a record pattern without `=`
binds the field's name, so its fix writes `{ name = _name }`.
