---
title: "FLICK-E007"
source_hash: "3647c91d5b856f25"
---

`try … with` обрабатывает ожидаемые ошибки: каждый вариант соответствует типу, объявленному с помощью
`type error`, или переменной либо `_`, которая соответствует любой ошибке тела. Другие
значения сопоставляются с `match` в результате.

```flick ignore
safe : UserId ->{Users; none} Option User
let safe id =
  try
    Some (Users.get id)
  with
  | NotFound _ -> None
```
