---
title: "FLICK-C008"
---

The superclasses of a class must not lead back to it. Remove one link of the
cycle.

```flick ignore
class Eq a =
  eq : a -> a -> Bool

class Eq a => Ord a =
  compare : a -> a -> Ordering
```
