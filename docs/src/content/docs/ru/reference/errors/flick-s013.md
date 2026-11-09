---
title: "FLICK-S013"
source_hash: "3741a3d1d0c7844a"
---

`<-` связывает значение только как оператор блока `do` или как часть `for`, где `x <- e` получает значение из `e` через `Monad.flat_map`. В других местах связывайте с помощью `let`. Для сравнения с отрицательным числом нужен пробел: `a < -1`, а не `a<-1`.

```flick
pair_optional : Option a -> Option b -> Option (a, b)
let pair_optional left right =
  do {
    x <- left
    y <- right
    pure (x, y)
  }
```
