---
title: "FLICK-C010"
source_hash: "7f0188228554aa93"
---

Метод экземпляра имеет сигнатуру метода класса с типами экземпляра вместо параметров класса. Он не может требовать ограничений, которых нет в сигнатуре класса.

```flick
# type Shape = Circle Float
class Show a =
  show : a -> String

instance Show Shape =
  show : Shape -> String
  let show shape = "shape"
```

Подпись по умолчанию — это повторение сигнатуры метода в теле класса с ограничениями, необходимыми по умолчанию. Она точно соответствует типу метода, ограничивает только параметры класса и собственные переменные, и является сигнатурой метода без собственных ограничений:

```flick
class Describe a =
  describe : a -> String
  describe : Show a => a -> String
  let describe x = show x
```
