---
title: "FLICK-S014"
---

An attribute is not `@lang` followed by one lang item the compiler knows.
`@lang` is the only attribute: it marks, in the built-in modules, a
declaration the compiler itself relies on, such as the type of integers.

```flick fail S014
@inline
double : Float -> Float
let double x = x * 2
```

A misspelled item, such as `@lang integer`, or another attribute, such as
`@inline`, is reported.
