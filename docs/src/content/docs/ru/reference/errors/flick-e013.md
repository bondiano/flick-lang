---
title: "FLICK-E013"
source_hash: "e191e0c7e52856a4"
---

Сервис — это эффект, все операции которого являются `tail`: обработчик задаёт значение каждой операции, а запись функций, `S.Impl`, является таким обработчиком.
Операция, которая никогда не возвращается (`abort`) или удерживает оставшуюся часть вычисления (`control`), должна находиться в `effect`.

```flick
effect Throw =
  // was, in `service Throw =`: abort throw : forall b. String ->{} b
  abort throw : forall b. String ->{} b
```
