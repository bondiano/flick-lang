---
title: "FLICK-C002"
---

A class constraint mentions a type that inference cannot determine, so no
instance can be chosen; candidates are listed. Annotate the value so its type
is known.

```flick fail C002
describe : Unit -> String
let describe () =
  let text = show []
  text
```
