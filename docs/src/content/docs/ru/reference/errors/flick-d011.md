---
title: "FLICK-D011"
source_hash: "c64ca6f1e98b6f08"
---

Часть или результат успешного выполнения endpoint-а записи, который наследует `HttpApi`, не имеет кодека. Его типу необходим один из следующих вариантов: значение с именем, соответствующим ему в snake_case в его модуле, которое является `PureCodec Json` этого типа (`credentials` для `Credentials`), `T.json` уточнённого типа, `String.json` встроенного типа или `deriving Schema`, чей JSON-кодек декодирует этот тип. Значение с таким именем, но другого типа, не является кодеком для него.

```flick
type alias Credentials = { authorization : Option String }
  deriving Schema
// was: a record without `deriving Schema` and no `credentials`
```
