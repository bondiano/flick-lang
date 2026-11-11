---
title: "FLICK-T071"
---

# Invalid loop control

`break` and `continue` need an enclosing `loop`, `while` or `for … do`
in the same function. Only `loop` accepts a value with `break value`.
A function, task, lazy sequence or operation handler cannot transfer control
into the loop around it.
