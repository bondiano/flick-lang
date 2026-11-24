---
title: "FLICK-F013"
---

A direct `foreign` declaration or `@native` override uses a parameter, result,
or callback type that is outside the active backend's versioned value-ABI table
(or used in a direction the table does not permit), and the type is not an
opaque handle.

Each backend publishes a value-ABI table in its descriptor mapping lang items
and structural type constructors (`tuple`, `function`) to host types, boundary
conversions (`none`, `copy`, `view`), and allowed directions (`in`, `out`,
`both`). Records and user-defined ADTs have no stable layout and stay outside
value-ABI tables.

Pass values using types admitted in the backend's value-ABI table, wrap an
unsupported value behind an opaque nominal `type` handle, or write an explicit
adapter:

```flick
type Handle

use_handle : Handle -> Int ->{} Unit
foreign use_handle from "./native.js" "useHandle"
  arity 2
```
