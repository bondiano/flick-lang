---
title: "FLICK-T028"
---

A warning: the `else` branch is just `()`, which an `if` without `else`
already is, so it can be removed without changing anything. The quick fix
deletes it; `flick check --fix` deletes all of them. Turn the warning off with
`allow = ["FLICK-T028"]` under `[lint]` in `flick.toml`.

```flick ignore
let warn msg =
  if verbose then
    Console.log msg
```
