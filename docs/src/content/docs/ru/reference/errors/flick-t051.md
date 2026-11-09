---
title: "FLICK-T051"
source_hash: "1a1d7f94aa029064"
---

`match`, у которого `Some x` ветвь возвращает `x`, а `None` ветвь возвращает литерал или имя, является `Option.with_default default o`. Быстрое исправление переписывает его. `None` ветвь, которая вычисляет значение или завершается с ошибкой, остается без изменений: `with_default` вычислил бы значение по умолчанию до `match`.

```flick
count : Option Float -> Float
let count o = Option.with_default 0 o
```
