---
title: "FLICK-C008"
source_hash: "1c7f40e87ea870be"
---

Суперклассы класса не должны ссылаться обратно на него. Удалите одну из ссылок, образующих цикл.

```flick
class Eq a =
  eq : a -> a -> Bool

class Eq a => Ord a =
  compare : a -> a -> Ordering
```
