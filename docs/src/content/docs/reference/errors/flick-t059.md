---
title: "FLICK-T059"
---

A signature writes something the compiler gives it anyway.

A foreign function with `retention owner` and one region in its signature gets
`Fiber.Outlives`, `Fiber.Provided` and `Detached`, so it need not write them:

```flick
add_finalizer : (Eff r none Unit)^c ->{Scope s | r} Unit
foreign effect add_finalizer from "./scope.js" "addFinalizer"
  retention owner
```

An instance method has the type its class gives it, so a signature that repeats
it can go. One whose type variables the method's body names stays, as they are
in scope there:

```flick fail T059
# type Box a = Box a
instance Functor Box =
  map : (forall v. a^v -> b)^c -> (Box a)^d -> (Box b)^(c + d)  // warns
  let map f (Box x) = Box (f x)
```

The fix is safe: `flick check --fix --only T059`.
