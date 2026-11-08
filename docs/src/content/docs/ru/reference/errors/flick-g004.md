---
title: "FLICK-G004"
source_hash: "a7e28d7e29891859"
---

Операция, которая требует `Scope s`, захватывает ресурс и регистрирует его освобождение у владельца региона `s`, который предоставляется только `scope { … }`. Вызовите это внутри области видимости или укажите требование в сигнатуре, чтобы область видимости вызывающего владела ресурсом.

```flick ignore
read_config : Path ->{FileSystem; OpenError, ReadError} String
let read_config path =
  scope {
    let file = File.open_scoped path
    File.read_all file
  }
```
