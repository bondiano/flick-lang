---
title: "FLICK-C014"
source_hash: "1a5690d9f881068c"
---

`@implicit` требует двух разных конкретных типов, отсутствия контекста и класса
с двумя параметрами и ровно одним чистым методом типа `a -> b` без дополнительных
ограничений.

```flick
class Convert a b =
  convert : a -> b

type Meters = Meters Float
type Feet = Feet Float

@implicit
instance Convert Meters Feet =
  let convert (Meters n) = Feet n
```
