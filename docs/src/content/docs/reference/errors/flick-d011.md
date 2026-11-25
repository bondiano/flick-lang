---
title: "FLICK-D011"
---

A part or the success of an endpoint of a record that derives `HttpApi`
has no codec. Its type needs one of: a value named after it in snake case in
its module that is a `PureCodec Json` of it (`credentials` for
`Credentials`), a refined type's `T.json`, a built-in type's `String.json`,
or `deriving Schema`, whose JSON codec decodes it. A value of that name with
another type is no codec of it.

```flick
type alias Credentials = { authorization : Option String }
  deriving Schema
// was: a record without `deriving Schema` and no `credentials`
```

The JSON codec of a schema writes `None` and `Some None` alike as `null`,
so a type with an `Option (Option a)` in it takes none from its schema: a
value named after it has to keep them apart, as `Codec.tagged_option` does.

```flick
type alias Patch = { bio : Option (Option String) }
  deriving Schema

patch : PureCodec Json Patch
let patch = codec Patch.schema {
  bio = field "bio" (Codec.tagged_option (Codec.tagged_option String.json))
}
// was: `Patch` with its schema alone
```
