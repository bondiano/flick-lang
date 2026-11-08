---
title: "FLICK-C003"
source_hash: "92ae363da68c48fc"
---

Два заголовка экземпляров описывают общее ограничение: после расширения алиасов их заголовки унифицируются. Экземпляры каноничны: нет приоритета между общим и конкретным, и контекст не выбирает. Оставьте один или дайте частному случаю номинальную оболочку.

```flick ignore
instance Show a => Show (List a) =
  show : List a -> String
  let show items = "[…]"

type Numbers = Numbers (List Float)

instance Show Numbers =
  show : Numbers -> String
  let show numbers = "numbers"
```
