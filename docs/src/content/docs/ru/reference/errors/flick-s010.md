---
title: "FLICK-S010"
source_hash: "66631c5cc1dd3b29"
---

Конструкция происходит из других языков ML, но в Flick её нет: блок `do` без фигурных скобок, операторы связывания `let*`, `of` перед полями конструктора и JSX `<…>`. Блок `do` требует фигурных скобок, `do { … }`; вызовы с эффектами записываются напрямую, отложенная работа использует `task { … }` и `await`, поля конструктора следуют за именем как в паттерне (`Rect Float Float`), UI использует `ui { … }`.

```flick
# service Users =
#   get : UserId ->{; NotFound} User
# type alias User = { name : String }
# type UserId = UserId Int
# type error NotFound = NotFound String
load_pair : UserId ->{Users; NotFound} (User, User)
let load_pair id =
  let first = Users.get id
  let second = await (task { Users.get id })
  (first, second)
```
