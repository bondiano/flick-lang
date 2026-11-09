---
title: "FLICK-T045"
---

The condition of this `if` is known before the program runs, so one branch
never runs. It is often left from debugging. The quick fix, not applied by
`flick check --fix`, keeps the branch that runs.

```flick
greeting : String
let greeting = "hello"
```
