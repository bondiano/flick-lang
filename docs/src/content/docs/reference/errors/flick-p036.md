---
title: "FLICK-P036"
---

A plugin gave no code for a block: it found the block wrong (its message
says why), it failed while it ran (it trapped, or ran out of its fuel or
memory), or the code it gave does not fit where the block stands. Errors
of the user's code inside the block are reported as such; this one is
about the plugin or how the block is written for it.

```flick
import Pairs exposing (plugin pairs)

let point = pairs { x = 1.0; y }
```
