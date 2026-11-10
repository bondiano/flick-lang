---
title: "FLICK-G001"
---

A qualifier says what using a value needs: calling a function `(τ → σ)^{s}`,
awaiting a task `(Eff r e a)^{s}` or running a stream `(Stream r e a)^{s}`
needs the region `s` live there. A narrower qualifier would hide that need,
so a value of `(Stream r e a)^{s}` cannot pass for a bare `Stream r e a`.
Widen the qualifier of the expected type, or use the value where its region
is live.

```flick
# type Handle (s : Region) = Handle
// was: same : Handle s -> (Stream.Stream none none Int)^{s} -> Stream.Stream none none Int
same : Handle s -> (Stream.Stream none none Int)^{s} -> (Stream.Stream none none Int)^{s}
let same _ stream = stream
```
