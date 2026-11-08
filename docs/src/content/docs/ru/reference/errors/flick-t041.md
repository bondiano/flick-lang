---
title: "FLICK-T041"
source_hash: "a7cfc3792e656846"
---

`if not c then a else b` заставляет ридер отменить `not`. С помощью `else`,
тот же самый `if` без `not` и с переставленными ветвями говорит это
напрямую. Быстрое исправление переписывает его, когда обе ветви помещаются в одну строку.

```flick ignore
label : Bool -> String
let label done = if done then "done" else "pending"
```
