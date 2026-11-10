---
title: "FLICK-G001"
source_hash: "04bb9cafa58655ca"
---

Квалификатор говорит, что нужно для использования значения: вызов функции `(τ → σ)^{s}`, ожидание задачи `(Eff r e a)^{s}` или запуск потока `(Stream r e a)^{s}` требует там живого региона `s`. Более узкий квалификатор скрыл бы эту потребность, поэтому значение `(Stream r e a)^{s}` нельзя передать как обычный `Stream r e a`. Расширьте квалификатор ожидаемого типа или используйте значение там, где его регион жив.

```flick
# type Handle (s : Region) = Handle
// was: same : Handle s -> (Stream.Stream none none Int)^{s} -> Stream.Stream none none Int
same : Handle s -> (Stream.Stream none none Int)^{s} -> (Stream.Stream none none Int)^{s}
let same _ stream = stream
```
