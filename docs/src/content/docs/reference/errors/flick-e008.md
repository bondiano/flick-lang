---
title: "FLICK-E008"
---

No longer reported: there is no build without a runtime, `[build] effect =
"none"`, any more. A program that runs nothing at runtime builds on Flick's
own runtime, which bundles only the helpers the program calls. The code
stays reserved.

```toml
# flick.toml: no `effect`, or the default
[build]
effect = "rt"
```
