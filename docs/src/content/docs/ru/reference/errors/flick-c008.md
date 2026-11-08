---
title: "FLICK-C008"
source_hash: "82f554473881432e"
---

Суперклассы класса не должны ссылаться обратно на него. Удалите одну из ссылок, образующих цикл.

```flick ignore
class Eq a =
  eq : a -> a -> Bool

class Eq a => Ord a =
  compare : a -> a -> Ordering
```
