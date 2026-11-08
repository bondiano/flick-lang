---
title: "FLICK-E006"
---

Running an operation of a service or an effect looks its handler up, so an
operation is a runtime function: at least one arrow of its signature is `->{…}`.
A pure helper belongs in a module, not in a service or an effect.

```flick ignore
service Clock =
  // was: now : Float
  now : Unit ->{} Float
```
