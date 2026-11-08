---
title: "FLICK-T015"
---

A recursive function calls itself at another type than the one being defined
(polymorphic recursion). The checker allows this only when the signature
quantifies all its variables with an explicit `forall`: then it checks the body
against the signature instead of inferring it.

```flick ignore
type Nested a = Flat a | Nest (Nested (List a))

depth : forall a. Nested a -> Float
let rec depth value =
  match value with
  | Flat _ -> 0
  | Nest inner -> 1 + depth inner
```
