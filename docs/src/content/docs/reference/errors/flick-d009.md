---
title: "FLICK-D009"
---

`deriving Url` maps each constructor of a variant type to a path, so the
type must derive `Schema` too, and each field must fit in a URL: a segment
of type `String`, `Float`, `Bool` or a refined type over them, or a final
record field of such fields, the query, where a field may be an `Option`.
A list, a nested variant type or a generic type has no URL form; the
message names the field.

```text
type Page = Home | User Float | Search { q : String; page : Option Float }
  deriving (Schema, Url)
// was: User { id : Float } (List String), a list is not a segment
```
