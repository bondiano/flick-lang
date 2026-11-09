---
title: "FLICK-T013"
---

Only values built purely are generalized. The result of a runtime call (even
`->{}`) is monomorphic, so a polymorphic signature, annotation or higher-rank
argument over it is rejected, and a top-level value whose type stays unknown
needs a signature. A signature with class constraints is rejected over it
too: its dictionaries would make the value a function, run again at each
use. Make a factory function, or describe the computation with `task` and
run it where each type is known.

```flick
make_empty : forall a. Unit ->{} Ref (Option a)
let make_empty () =
  Ref.make None

fresh_cell : forall a. Eff none none (Ref (Option a))
let fresh_cell =
  task { Ref.make None }
```
