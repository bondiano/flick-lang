---
title: "FLICK-G001"
---

A closure or a `task` holds the values it uses. When one of them belongs to
a region, the closure holds that region too, and its type must say so with a
capture qualifier: `T^{s}`. A bare type holds no region it does not show, so
a closure that uses a scoped file cannot pass for `Unit -> String`. Widen the
qualifier of the expected type, or use the resource where it lives.

```flick ignore
// was: read_later : forall s. File s -> Eff none {ReadError} String
read_later : forall s. File s -> (Eff none {ReadError} String)^{s}
let read_later file =
  task { File.read_all file }
```
