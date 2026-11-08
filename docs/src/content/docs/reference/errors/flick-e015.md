---
title: "FLICK-E015"
---

The `return` arm of a handler takes the value of its body when the body
finishes. A handler has at most one, it has no guard, and its pattern
matches every value: the value has nowhere else to go.

```text
collect : forall a r e. (Unit ->{Emit a | r; e} Unit) ->{r; e} List a
let collect body =
  try
    body ()
  with
  | Emit.emit x -> x :: resume ()
  // was: | return () when False -> []
  | return () -> []
```
