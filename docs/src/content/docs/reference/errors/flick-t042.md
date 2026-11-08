---
title: "FLICK-T042"
---

An `else` whose only expression is another `if` nests a level for
nothing: `else if` continues the chain. The quick fix joins them.

```flick ignore
sign : Float -> String
let sign n = if n > 0 then "+" else if n < 0 then "-" else "0"
```
