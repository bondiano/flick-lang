---
title: "FLICK-D006"
source_hash: "f21b060803d75c30"
---

`T.parse` константы проверяется во время компиляции программы, если компилятор
знает проверку: это `Checks` функция или `fun` над константой, построенной из литералов. Константа, которая не проходит проверку, не может стать `T`.

```flick
refined type Port = Float where fun n -> n > 0 && n < 65536

default_port : Result SchemaError Port
let default_port = Port.parse 8080
```
