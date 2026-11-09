---
title: "FLICK-C007"
source_hash: "0ef1f25637082a36"
---

Разрешение экземпляра должно завершаться: каждая переменная типа в контексте экземпляра
должна появляться в сигнатуре, и каждое ограничение контекста должно быть меньше сигнатуры,
без повторения переменной чаще. `Eq a => Eq (List a)` корректно.

```flick
# type Wrap a = Wrap a
// not `instance Show (List a) => Show (Wrap a)`
instance Show a => Show (Wrap a) =
  show : Wrap a -> String
  let show w =
    match w with
    | Wrap x -> show x
```
