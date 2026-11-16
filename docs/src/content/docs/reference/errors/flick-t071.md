---
title: "FLICK-T071"
---

# Invalid loop control

`break` and `continue` need an enclosing `loop`, `while` or `for … do`
in the same function (and matching label when `'label` is given).
Loop labels must be unique within the same function scope, and only `loop`
accepts a value with `break value`. A function, task, lazy sequence or
operation handler cannot transfer control into the loop around it.
