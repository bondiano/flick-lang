---
title: "FLICK-T019"
---

The last statement of a block is its value, so a block cannot end with a `let`
or a signature. Add the expression the block evaluates to.

```flick
double : Float -> Float
let double n =
  let twice = n * 2
  twice
```
