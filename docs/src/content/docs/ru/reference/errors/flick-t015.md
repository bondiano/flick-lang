---
title: "FLICK-T015"
source_hash: "8f6e555700ad5621"
---

Рекурсивная функция вызывает сама себя с другим типом, чем тот, который определяется (полиморфная рекурсия). Проверяющий разрешает это только в случае, если сигнатура явно квантифицирует все свои переменные с помощью `forall`: тогда тело проверяется в соответствии с сигнатурой, а не выводится.

```flick
type Nested a = Flat a | Nest (Nested (List a))

depth : forall a. Nested a -> Float
let rec depth value =
  match value with
  | Flat _ -> 0
  | Nest inner -> 1 + depth inner
```
