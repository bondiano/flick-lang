---
title: "FLICK-C004"
source_hash: "e0c2b6060336867e"
---

При функциональной зависимости `c -> a` типы в `c` однозначно определяют типы в `a`: два экземпляра, у которых определяющие типы унифицируются, должны совпадать и по остальным типам.  
Оберните один из типов, чтобы сохранить обе зависимости.

```flick
# type User = User String
# type UserSet = UserSet (List User)
class Collection c a | c -> a =
  empty : c

instance Collection UserSet User =
  empty : UserSet
  let empty = UserSet []
```
