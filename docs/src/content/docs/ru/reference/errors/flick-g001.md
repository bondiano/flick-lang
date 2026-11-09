---
title: "FLICK-G001"
source_hash: "367d08f5871edf96"
---

Замыкание или `task` содержит значения, которые оно использует. Если одно из них принадлежит региону, замыкание также содержит этот регион, и его тип должен это указывать с помощью квалификатора захвата: `T^{s}`. Обычный тип не содержит регион, который он не показывает, поэтому замыкание, использующее локальный файл, не может быть передано как `Unit -> String`. Расширьте квалификатор ожидаемого типа или используйте ресурс там, где он находится.

```flick
# import File exposing (File, OpenError, ReadError, FileSystem)
// was: read_later : forall s. File s -> Eff none {ReadError} String
read_later : forall s. File s -> (Eff {FileSystem, Async} {ReadError} String)^{s}
let read_later file =
  task { File.read_all file }
```
