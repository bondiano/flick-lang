---
title: "FLICK-C013"
source_hash: "1800a1f75b210598"
---

`deriving` охватывает `Eq`, `Ord`, `Hash` и `Show` номинальных типов. Каждое поле требует экземпляра класса, а `Hash` и `Ord` выводятся вместе с `Eq`, поэтому все они используют одни и те же поля. Поле, такое как `Ref`, не имеет `Hash`: напишите экземпляр вручную или оберните поле.

```flick
type Point = Point Float Float
  deriving Eq, Hash, Show
```
