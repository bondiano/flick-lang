---
title: "FLICK-T041"
source_hash: "0646c360ad00063b"
---

`if not c then a else b` заставляет ридер отменить `not`. С помощью `else`,
тот же самый `if` без `not` и с переставленными ветвями говорит это
напрямую. Быстрое исправление переписывает его, когда обе ветви помещаются в одну строку.

```flick
label : Bool -> String
let label done = if done then "done" else "pending"
```
