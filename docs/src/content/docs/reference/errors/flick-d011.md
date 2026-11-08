---
title: "FLICK-D011"
---

A part or the success of an endpoint of a record that derives `HttpApi`
has no codec. Its type needs one of: a value named after it in snake case in
its module that is a `PureCodec Json` of it (`credentials` for
`Credentials`), a refined type's `T.json`, a built-in type's `String.json`,
or `deriving Schema`, whose JSON codec decodes it. A value of that name with
another type is no codec of it.

```flick ignore
type alias Credentials = { authorization : Option String }
  deriving Schema
// was: a record without `deriving Schema` and no `credentials`
```
