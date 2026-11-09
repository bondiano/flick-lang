---
title: "FLICK-F004"
source_hash: "7ac0d80ffd953b8b"
---

`--emit-dts` генерирует объявление TypeScript для каждого экспорта и отказывается писать ложное: переменная типа высшего порядка, значение, содержащее регион, открытая запись или реализация сервиса не имеют достоверной формы в TypeScript. Экспортируйте конкретную функцию вместо этого (фасад) или не включайте значение в экспорт модуля.

```flick
// was: map_twice : Functor f => (a -> a) -> f a -> f a
map_twice_list : (a -> a) -> List a -> List a
let map_twice_list f xs = List.map f (List.map f xs)
```
