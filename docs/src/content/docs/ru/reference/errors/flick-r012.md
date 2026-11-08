---
title: "FLICK-R012"
source_hash: "5bbf18d74a54926b"
---

Модуль импортируется, или имя импортируется с помощью `exposing`, но ничего в модуле его не использует. Исправление предупреждения удаляет импорт или имя.

```flick ignore
module App.Report exposing (total)

import List exposing (map, filter)

total : List Float -> Float
let total xs = List.sum (map (fun x -> x * 2) xs)
```

`filter` никогда не используется. Импорт, модуль которого объявляет экземпляры, сохраняется: импорт может быть важен только для его экземпляров.
