---
title: "FLICK-T021"
---

A warning: the `let` annotation names exactly the type its value already
has on its own, so it can be removed without changing anything. The quick
fix deletes it; `flick check --fix` deletes all of them. Turn the warning off with `allow = ["FLICK-T021"]` under
`[lint]` in `flick.toml`.

```flick ignore
let kinds = [Tracer.internal, Tracer.server]
```
