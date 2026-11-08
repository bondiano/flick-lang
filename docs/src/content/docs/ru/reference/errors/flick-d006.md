---
title: "FLICK-D006"
source_hash: "b4f11b32ae35f4cb"
---

`T.parse` константы проверяется во время компиляции программы, если компилятор
знает проверку: это `Checks` функция или `fun` над константой, построенной из литералов. Константа, которая не проходит проверку, не может стать `T`.

```flick ignore
refined type Port = Float where fun n -> n > 0 && n < 65536

default_port : Result SchemaError Port
let default_port = Port.parse 8080
```
