---
title: "FLICK-T012"
source_hash: "b77a9677d15a77c7"
---

Значение будет вычислено в процессе его определения. В этой версии только функции могут быть рекурсивными: `let rec` и `rec { … }` определяют функции, а значения верхнего уровня не должны зависеть друг от друга циклично.

```flick
# countdown : Float -> List Float
let rec countdown n =
  if n <= 0 then [] else [n] ++ countdown (n - 1)
```
