---
title: "FLICK-S014"
---

An attribute is unknown, has invalid arguments, or is misplaced.
The core attributes are `@lang`, `@derive`, `@maker`, `@cell`, `@label`, `@owner`,
`@sync`, `@stores`, `@implicit` and `@when`, plus any type-form attributes
declared by the active backend (such as `@ts` on `flick/js`). `@lang` names one compiler lang item; `@derive` optionally names
a plugin package; `@maker` takes no argument; `@cell` takes no argument and
belongs before a type declaration; `@label` names one kind, as `@label atom`,
and belongs before a type declaration; `@owner` takes no argument and belongs
before a service declaration; `@sync` takes `reader` or nothing and belongs
before a service declaration; `@ts` (declared by `flick/js`) takes the TypeScript form of a type, as
`@ts "ReadonlyArray<{0}>"`, `none`, or `import` with a module and an export
of it, and belongs before the declaration of a type or a service, with one
form that can be written for the type; `@stores` takes no argument and
belongs before a type declaration without constructors or a body;
`@implicit` takes no argument and belongs only
before an instance declaration; `@when` takes a condition string before a module
header.

```flick fail S014
@inline
double : Float -> Float
let double x = x * 2
```

A misspelled item, such as `@lang integer`, an attribute not declared by the
active backend, or an unknown attribute, such as `@inline`, is reported.
