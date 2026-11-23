---
title: "E-AWAITABLE-MISSING"
source_hash: "40a5bd4d04ce9731"
---

Для операнда `await` нужен instance класса `Awaitable`. Во Flick есть
встроенные instances для `Eff` и `Lazy`. Если тип операнда неизвестен,
укажите его явно или добавьте ограничение `Awaitable`.

```flick fail E-AWAITABLE-MISSING
let program = task { await 42 }
```

Задача может выполнить через await другую задачу и использовать её результат.

```flick
let program = task { await (task { 42 }) }
```
