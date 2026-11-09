---
title: "FLICK-T008"
source_hash: "88e85fde9b8909bd"
---

Сигнатура обещает полиморфный тип, а реализация работает только для одного конкретного типа. В `forall a. a -> a` вызывающий выбирает `a`, поэтому тело не может прибавлять число к своему аргументу или вызывать на нём функцию `String`. Используйте менее общую сигнатуру или более общую реализацию.

```flick
apply_twice : (forall a. a -> a) -> (Float, String)
let apply_twice transform =
  (transform 42, transform "hello")

keep : forall a. a -> a
let keep value = value
```
