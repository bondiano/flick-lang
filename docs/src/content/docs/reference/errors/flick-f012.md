---
title: "FLICK-F012"
---

Two or more `@native` overrides on the same top-level function match the active
environment, and neither override's `when` condition implies the other over the
known environment tag vocabulary.

When multiple `@native` clauses match the active environment, Flick selects the
most specific one: the clause whose `when` condition implies every other
matching clause (for example, `bun` implies `js`, and `js && rt` implies `js`).
If two matching clauses are incomparable or logically equivalent, Flick refuses
to guess and reports both override lines.

Make one condition strictly more specific than the other, or make their
conditions disjoint:

```flick
# import Float
#
hypot : Float -> Float -> Float
let hypot a b = Float.sqrt (a * a + b * b)
  @native "Math.hypot" when js && !bun
  @native "Bun.hypot" when bun
```
