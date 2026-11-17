---
title: "FLICK-T075"
source_hash: "4f371237c5b0fe42"
---

Предупреждение: `from_result (Err e)` строит `Result` только для того, чтобы завершиться с его ошибкой. `Fail.fail e` завершается с ошибкой `e` напрямую и имеет тот же тип, `Errors e ->{; e} a`, в любой функции, строка которой допускает `e`. Быстрое исправление записывает его; `flick check --fix` применяет его.

```flick
# type error NotFound = NotFound Int
# find : Int -> Option String
# let find id = None
name : Int ->{; NotFound} String
let name id =
  match find id with
  | Some found -> found
  | None -> Fail.fail (NotFound id)
```
