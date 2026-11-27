---
title: "FLICK-F017"
---

A constructor of a type with `@native when <cond>` is built or matched where the
type may be the backend's own.

Where the type's condition holds, its values come from the backend's bindings
and have no constructors to match. Only the Flick body of a definition whose
`@native` clauses hold wherever the type's does may name them: there a native
clause replaces the body, so the body runs only where the constructors are the
type's representation. Anything else, a definition without `@native`, an
instance method or a derived instance, could meet a value it cannot read.

Give the definition a `@native` clause that covers the type's condition, or
reach the representation through such a definition:

```flick
type Box a =
  | Box a
  @native when js || go

make : a -> Box a
let make x = Box x
  @native "./box.mjs" "make" when js || go
```
