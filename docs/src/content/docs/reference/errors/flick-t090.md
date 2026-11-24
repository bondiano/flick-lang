---
title: "FLICK-T090"
---

A proof step asks for an equality whose sides compute to different values
or distinct types. The message shows the equality required at that step.
In this example, the first argument of `trans` chooses `false` as the
middle value, so the first `refl` must prove `true = false`.

```flick fail T090
proof wrong_step where equal true true = trans false refl refl
```

Prove the stated equality directly.

```flick
proof right_step where equal true true = refl
```
