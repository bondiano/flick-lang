---
title: "FLICK-S014"
---

An attribute is unknown, has invalid arguments, or is misplaced.
The attributes are `@lang`, `@derive`, `@maker` and `@implicit`.
`@lang` names one compiler lang item; `@derive` optionally names a plugin
package; `@maker` takes no argument; `@implicit` takes no argument and belongs
only before an instance declaration.

```flick fail S014
@inline
double : Float -> Float
let double x = x * 2
```

A misspelled item, such as `@lang integer`, or an unknown attribute, such as
`@inline`, is reported.
