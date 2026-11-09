---
title: "FLICK-D010"
---

`deriving Form` gives a record type a text input per field, so the type
must derive `Schema` too, and each field must be typed from text: a
`String`, a `Float`, a `Bool`, a refined type over them, or an `Option` of
one, which is `None` when its input is empty. A list, a nested record or a
variant type has no input; the message names the field.

```flick
type alias Signup = { name : String; age : Option Float }
  deriving Schema, Form
// was: tags : List String, a list has no single input
```
