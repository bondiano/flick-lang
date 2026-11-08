---
title: "FLICK-C013"
source_hash: "9e35d024290ed2ff"
---

`deriving` охватывает `Eq`, `Ord`, `Hash` и `Show` номинальных типов. Каждое поле требует экземпляра класса, а `Hash` и `Ord` выводятся вместе с `Eq`, поэтому все они используют одни и те же поля. Поле, такое как `Ref`, не имеет `Hash`: напишите экземпляр вручную или оберните поле.

```flick ignore
type Point = Point Float Float
  deriving Eq, Hash, Show
```
