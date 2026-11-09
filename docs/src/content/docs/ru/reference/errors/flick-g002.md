---
title: "FLICK-G002"
source_hash: "fea93a2f92ece628"
---

`scope { … }` освобождает то, что было приобретено, когда его тело заканчивается. Значение, которое содержит регион области видимости (дескриптор, замыкание или `task`, использующее его), не может быть возвращено из области видимости, сохранено в ячейке, созданной вне её, или выброшено как ошибка. Завершите работу внутри области видимости или переместите область в отложенную работу, чтобы каждый запуск приобретал собственный ресурс.

```flick ignore
// was: scope { let file = File.open_scoped path
//              task { File.read_all file } }
deferred_read : Path -> Eff {FileSystem} {OpenError, ReadError} String
let deferred_read path =
  task {
    scope {
      let file = File.open_scoped path
      File.read_all file
    }
  }
```
