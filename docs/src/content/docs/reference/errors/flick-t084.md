---
title: "FLICK-T084"
---

Checking a `proof` computes its two sides within a fixed budget of steps
and depth, the same on every platform. This proof needed more: nothing
is proved, even of a true statement, and a proof that runs out is never
taken as true. State the property of smaller terms.

```flick fail T084
proof costly (f : Bool -> Bool) (x : Bool)
where equal ((fun t -> t (t (t (t (t (t (t (t (t f))))))))) (fun g y -> g (g y)) x) ((fun t -> t (t (t (t (t (t (t (t (t f))))))))) (fun g y -> g (g y)) x) =
  refl
```

is

```flick
proof cheap (f : Bool -> Bool) (x : Bool)
where equal ((fun g y -> g (g y)) f x) (f (f x)) =
  refl
```
