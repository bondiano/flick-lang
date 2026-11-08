---
title: "FLICK-P040"
---

Two packages of the project's dependencies name one npm package at different
versions in `[js.dependencies]`, or one of them differs from the project's
`package.json`. A project installs one copy of an npm package, so the
requirements must be the same text: change the project's `package.json`, or
use versions of the packages that agree.

```toml
[js.dependencies]
left-pad = "^1.3.0"
```
