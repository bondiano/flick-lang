---
title: "FLICK-R006"
source_hash: "15d6f268dcc30272"
---

Модули импортируют друг друга по циклу; сообщение содержит цепочку. В Flick нет циклических импортов между модулями. Переместите общие определения в модуль, который могут импортировать оба, или объедините модули.

```flick ignore
// src/App/Types.fl holds what App.Users and App.Orders both need
module App.Types exposing (UserId, OrderId)
```
