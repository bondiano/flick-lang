---
title: "FLICK-C010"
---

An instance method has the signature of its class method with the instance's
types in place of the class parameters. It may not require constraints the
class signature does not.

```flick ignore
class Show a =
  show : a -> String

instance Show Shape =
  show : Shape -> String
  let show shape = "shape"
```

A default signature, the class body's repeat of a method's signature with
constraints its default needs, has the method's type exactly, constrains
only the class's parameters and variables of its own, and is the one of a
method without constraints of its own:

```flick ignore
class Describe a =
  describe : a -> String
  describe : Show a => a -> String
  let describe x = show x
```
