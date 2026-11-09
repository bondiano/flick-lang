---
title: "FLICK-R008"
---

A module defines a name twice in one namespace: two values, two types or two
constructors with the same name. A signature and its `let` are one
definition. Rename or remove one of them.

```flick fail R008
type Light = Green | Red
type Stop = Red | Amber
```
