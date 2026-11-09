---
title: "FLICK-G004"
source_hash: "5241920fecc2ce8b"
---

Операция, которая требует `Scope s`, захватывает ресурс и регистрирует его освобождение у владельца региона `s`, который предоставляется только `scope { … }`. Вызовите это внутри области видимости или укажите требование в сигнатуре, чтобы область видимости вызывающего владела ресурсом.

```flick
# import File exposing (File, Path, OpenError, ReadError, FileSystem)
read_config : Path ->{FileSystem, Async; OpenError, ReadError} String
let read_config path =
  scope {
    let file = File.open_scoped path
    File.read_all file
  }
```
