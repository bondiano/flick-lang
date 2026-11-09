---
title: "FLICK-T002"
source_hash: "62e1dd43ff1fa199"
---

Тип используется там, где ожидается тип другого вида. `Eff` принимает сервисный ряд, ряд ошибок и тип: `Eff Unit Unit a` отклоняется, потому что `Unit` является `Type`, а не рядом. Сервисные ряды, ряды ошибок и ряды захвата — это разные виды и не смешиваются; ряд ошибок содержит только типы, объявленные с помощью `error`; конструктор типа принимает столько аргументов, сколько позволяет его вид.

```flick ignore
# service Users =
#   get : UserId ->{; NotFound} User
#   touch : UserId ->{} Unit
# type UserId = UserId String
# type User = User { name : String }
# type error NotFound = NotFound String
load : UserId -> Eff {Users} {NotFound} User   // not `Eff Unit Unit User`
```
