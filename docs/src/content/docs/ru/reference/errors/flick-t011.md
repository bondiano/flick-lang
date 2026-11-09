---
title: "FLICK-T011"
source_hash: "131f4704785e2765"
---

Выражение в середине блока должно иметь тип `Unit`; результат другого типа не отбрасывается автоматически. Свяжите его с `let` или явно отбросьте с помощью `ignore`.

```flick
# service Users =
#   get : UserId ->{; NotFound} User
#   touch : UserId ->{} Unit
# type UserId = UserId String
# type User = User { name : String }
# type error NotFound = NotFound String
refresh : UserId ->{Users; NotFound} Unit
let refresh id =
  ignore (Users.get id)
  Users.touch id
```
