---
title: "FLICK-T035"
---

A warning: nested `flat_map` (or `Option.bind`, `Result.bind`) calls whose
functions are `fun x -> …`, and `match`es that pass `None` or `Err e` on,
bind one value after another. A `do` block says the same in order:

```flick ignore
total : Option Float -> Option Float -> Option Float
let total a b =
  Option.bind (fun x -> Option.bind (fun y -> Some (x + y)) b) a
```

is

```flick ignore
total : Option Float -> Option Float -> Option Float
let total a b = do { x <- a; y <- b; Some (x + y) }
```

The quick fix rewrites a chain of two steps or more whose parts fit on one
line; `flick check --fix` applies it.
