---
title: "FLICK-D001"
source_hash: "fadc66ae9c0beb32"
---

`deriving Schema` строит свидетельство из определения типа, и для каждого поля требуется такое свидетельство: примитив, тип, который сам наследует `Schema`, `List`, `Option`, `Result`, `Redacted`, кортеж или запись из них, либо параметр типа. Функция, `Ref`, ограниченный ресурс или описание времени выполнения свидетельства не имеют; в сообщении указано имя пути поля. `T.schema` требует тип, который наследует `Schema`.

```flick ignore
type alias Profile = { name : String; session_id : String }
  deriving Schema
// was: session : File s, a live resource with no wire form
```
