---
title: "FLICK-T036"
---

A function whose body is longer than `[lint] max-function-lines` in
`flick.toml` (60 by default) does too much to read at once. Split it into
functions that each do one part, and name each part.

```flick ignore
summary : Order -> String
let summary order = header order ++ "\n" ++ lines order ++ "\n" ++ total order
```

Raise the limit with `max-function-lines = 100` under `[lint]`, or turn the
warning off with `allow = ["FLICK-T036"]`.
