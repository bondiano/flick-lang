---
title: "FLICK-S013"
---

`<-` binds a value only as a statement of a `do` block or a clause of `for`, where `x <- e` takes
the value out of `e` through `Monad.flat_map`. Elsewhere bind with `let`. A
comparison with a negative number needs a space: `a < -1`, not `a<-1`.

```flick
pair_optional : Option a -> Option b -> Option (a, b)
let pair_optional left right =
  do {
    x <- left
    y <- right
    pure (x, y)
  }
```
