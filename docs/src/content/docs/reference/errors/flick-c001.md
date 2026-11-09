---
title: "FLICK-C001"
---

A class constraint needs an instance, and none applies: the type has no
instance of the class, or the constraint is about a type variable that the
signature does not constrain. Declare the instance, or add the constraint to
the signature.

```flick
render : Show a => a -> String
let render value =
  show value
```
