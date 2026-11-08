---
title: "FLICK-C011"
source_hash: "37e38bfaa74ff24a"
---

Экземпляр определяет ровно те методы, что есть у его класса: каждый метод ровно один раз и ничего больше.

```flick ignore
instance Show Shape =
  show : Shape -> String
  let show shape = "shape"
```

В теле класса `let` является реализацией по умолчанию одного из методов класса, а сигнатура по умолчанию, повторяющая сигнатуру метода, сопровождается её `let`:

```flick ignore
class Describe a =
  describe : a -> String
  describe : Show a => a -> String
  let describe x = show x
```
