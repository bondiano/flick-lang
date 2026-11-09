---
title: "FLICK-D010"
source_hash: "d0e254c712dec7b4"
---

`deriving Form` задаёт тип записи с текстовым вводом для каждого поля, поэтому тип должен также наследовать `Schema`, и каждое поле должно иметь тип, основанный на тексте: `String`, `Float`, `Bool`, уточнённый тип на их основе или `Option` одного из них, который равен `None`, если его ввод пуст. Список, вложенная запись или вариантный тип не имеют ввода; сообщение указывает имя поля.

```flick
type alias Signup = { name : String; age : Option Float }
  deriving Schema, Form
// was: tags : List String, a list has no single input
```
