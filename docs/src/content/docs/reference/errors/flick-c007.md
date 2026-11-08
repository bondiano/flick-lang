---
title: "FLICK-C007"
---

Instance resolution must end: every type variable of an instance context
appears in the head, and each context constraint is smaller than the head
without repeating a variable more often. `Eq a => Eq (List a)` is fine.

```flick ignore
// not `instance Show (List a) => Show (Wrap a)`
instance Show a => Show (Wrap a) =
  show : Wrap a -> String
  let show w =
    match w with
    | Wrap x -> show x
```
