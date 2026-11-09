---
title: "FLICK-C012"
source_hash: "f0a0c5f378b38774"
---

Некоторые ограничения решаются компилятором: примитивы строк `Record`
(`HasField`, `Lacks`, `Remove`, `Project`, `UnionDisjoint`, `Map`, `Labels`)
и `Eq`, `Ord`, `Hash`, `Show` записей и кортежей, которыми делится каждый алиас
одинаковой формы. Объявите номинальный тип, чтобы дать ему собственный экземпляр.
Классы `Numeric` и `Iterable` закрыты: их экземпляры —
встроенные, так как компилятор генерирует их операции, а `for` циклы
напрямую.

```flick ignore
type User = User { name : String }

instance Show User =
  show : User -> String
  let show user =
    match user with
    | User fields -> fields.name
```
