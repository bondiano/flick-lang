---
title: "FLICK-C012"
---

Some constraints are solved by the compiler: the row primitives of `Record`
(`HasField`, `Lacks`, `Remove`, `Project`, `UnionDisjoint`, `Map`, `Labels`)
and `Eq`, `Ord`, `Hash`, `Show` of records and tuples, which every alias of
the same form shares. Declare a nominal type to give it its own instance.
The classes `Numeric` and `Iterable` are closed: their instances are the
built-in ones, since the compiler emits their operations and `for` loops
directly.

```flick ignore
type User = User { name : String }

instance Show User =
  show : User -> String
  let show user =
    match user with
    | User fields -> fields.name
```
