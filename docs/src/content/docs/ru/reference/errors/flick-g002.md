---
title: "FLICK-G002"
source_hash: "f209a6f8de644ad0"
---

`scope { … }` освобождает то, что было приобретено, когда его тело заканчивается, поэтому регион жив только внутри своей области. Функцию или `task`, которые используют ресурс области, нельзя вернуть из неё или сохранить в ячейке, созданной вне её: для их использования там нужен живой регион. Значение, тип которого называет регион, вообще не может покинуть область, и ошибка не может его нести. Значение, которое только держит дескриптор и никогда его не использует, может выйти. Завершите работу внутри области или переместите область в отложенную работу, чтобы каждый запуск приобретал собственный ресурс.

```flick
# import File exposing (File, Path, OpenError, ReadError, FileSystem)
// was: scope { let file = File.open_scoped path
//              task { File.read_all file } }
deferred_read : Path -> Eff {FileSystem, Async} {OpenError, ReadError} String
let deferred_read path =
  task {
    scope {
      let file = File.open_scoped path
      File.read_all file
    }
  }
```
