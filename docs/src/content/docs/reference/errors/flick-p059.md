---
title: "FLICK-P059"
---

`--target <name>` or `[build] target = "<name>"` named a backend that is
neither an official target (`js`, `go`) nor the short name or id of the
project's `[build.backend]` pin.

Use `js` or `go`, or pin the third-party backend component under
`[build.backend]` in `flick.toml`.

```toml
[build]
target = "go"
```
