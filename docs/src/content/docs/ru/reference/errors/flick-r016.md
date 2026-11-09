---
title: "FLICK-R016"
source_hash: "bde0c47276da90f3"
---

Модуль программы помечает объявление атрибутом `@lang`. Lang items — это то, на что полагается компилятор, например, `Option` для `?` или `flat_map` для `do`, и только встроенные модули их объявляют. Удалите этот атрибут: собственным типам и функциям программы он не нужен.

```flick
module App.Ids exposing (UserId)

type UserId = UserId Int
```
