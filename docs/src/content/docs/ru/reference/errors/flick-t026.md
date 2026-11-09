---
title: "FLICK-T026"
source_hash: "2b4181877fc97e6b"
---

`let assert` останавливает программу, если его шаблон не совпадает. Значение верхнего уровня вычисляется при загрузке модуля, и модуль не может завершиться с ошибкой во время загрузки: поместите `let assert` внутри блока функции или `match` поверх значения.

```flick ignore
module App.Config exposing (port)

port : List Float -> Float
let port ports =
  let assert [first, ..._] = ports as "no port configured"
  first
```
