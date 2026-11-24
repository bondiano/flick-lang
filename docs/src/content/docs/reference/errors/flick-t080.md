---
title: "FLICK-T080"
---

A `proof` without premises states that its two sides are equal for every
value of its parameters. Here the checker found an input where the sides
are different, or a `same_type` statement compared distinct types.
Check that the statement says what you mean.

An equality introduced by a step such as `trans` has its own diagnostic,
FLICK-T090. That equality can fail even when the statement holds.

```flick fail T080
proof always_some (x : Bool)
where equal (Some x) None =
  refl
```

is

```flick
proof always_some (x : Bool)
where equal (Some x) (Some x) =
  refl
```
