---
title: "FLICK-P024"
---

`flick pack` refuses a package whose modules still have `todo` or `echo`:
a `todo` fails when it runs, and an `echo` writes to stderr of whoever uses
the package. Each one is shown as an error; finish or remove it, then pack
again.

```flick ignore
render : List Float -> String
let render rows = todo "render the rows as a table"
```
