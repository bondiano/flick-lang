---
title: "FLICK-T011"
source_hash: "9e4130416a0a0687"
---

Выражение в середине блока должно иметь тип `Unit`; результат другого типа не отбрасывается автоматически. Свяжите его с `let` или явно отбросьте с помощью `ignore`.

```flick ignore
refresh : UserId ->{Users; NotFound} Unit
let refresh id =
  ignore (Users.get id)
  Users.touch id
```
