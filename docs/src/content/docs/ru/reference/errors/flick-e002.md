---
title: "FLICK-E002"
source_hash: "9795624440f88a3a"
---

Сигнатура — это верхняя граница сервисов и ошибок, необходимых её телу: тело может требовать меньше, но никогда не больше. То же самое относится к значению или `task`, проверяемым на соответствие ожидаемому типу. Добавьте отсутствующее в сигнатуру, предоставьте сервис или обработайте ошибку с помощью `try … with`.

```flick ignore
// was: load_name : UserId ->{Users; NotFound} String
load_name : UserId ->{Users, Log; NotFound} String
let load_name id =
  let user = Users.get id
  Log.info user.name
  user.name
```
