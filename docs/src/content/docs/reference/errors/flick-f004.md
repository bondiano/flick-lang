---
title: "FLICK-F004"
---

`--emit-dts` writes a TypeScript declaration for every export, and refuses
to write a false one: a higher-kinded type variable, a value holding a
region, an open record or a service implementation has no faithful
TypeScript form. Export a concrete function instead (a façade), or leave the
value out of the module's exports.

```flick ignore
// was: map_twice : Functor f => (a -> a) -> f a -> f a
map_twice_list : (a -> a) -> List a -> List a
let map_twice_list f xs = List.map f (List.map f xs)
```
