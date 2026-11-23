---
title: "E-AWAITABLE-MISSING"
---

The operand of `await` needs an `Awaitable` instance. Flick provides
instances for `Eff` and `Lazy`. If the operand's type is unknown, give it
a type annotation or declare the required `Awaitable` constraint.

```flick fail E-AWAITABLE-MISSING
let program = task { await 42 }
```

A task can await another task and use its result.

```flick
let program = task { await (task { 42 }) }
```
