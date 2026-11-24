---
title: "FLICK-T090"
source_hash: "d1125926dad1eda0"
---

Шаг доказательства требует равенства, стороны которого вычисляются в разные значения или обозначают разные типы. Сообщение показывает равенство, которое нужно доказать на этом шаге. В примере первый аргумент `trans` выбирает `false` как промежуточное значение, поэтому первый `refl` должен доказать `true = false`.

```flick fail T090
proof wrong_step where equal true true = trans false refl refl
```

Докажите исходное равенство напрямую.

```flick
proof right_step where equal true true = refl
```
