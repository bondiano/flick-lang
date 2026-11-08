---
title: "FLICK-T051"
---

A `match` whose `Some x` arm returns `x` and whose `None` arm returns a
literal or a name is `Option.with_default default o`. The quick fix
rewrites it. A `None` arm that computes or fails is left alone:
`with_default` would compute its default before the `match`.

```flick ignore
count : Option Float -> Float
let count o = Option.with_default 0 o
```
