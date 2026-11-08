---
title: "FLICK-T037"
---

An `if`, `match`, `fun`, `do`, `task`, `for` or `try` nested deeper than
`[lint] max-nesting` in `flick.toml` (4 by default). An `else if` does not
nest. Move the inner part into a function of its own, or match on several
values at once.

```flick ignore
label : Option User -> String
let label user = match user with
  | Some u -> named u
  | None -> "anonymous"
```

Raise the limit with `max-nesting = 6` under `[lint]`, or turn the warning
off with `allow = ["FLICK-T037"]`.
