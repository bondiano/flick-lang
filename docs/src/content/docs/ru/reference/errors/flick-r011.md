---
title: "FLICK-R011"
source_hash: "c7930c1f315c0b44"
---

Список `exposing` в заголовке модуля указывает на что-то, что модуль ни определяет, ни импортирует, или использует `Type(..)` для типа без конструкторов для экспорта: псевдоним или непрозрачный тип, представление которого остаётся приватным.

```flick fail R011
module App.Ids exposing (UserId, user_id)

opaque type alias UserId = String
```
