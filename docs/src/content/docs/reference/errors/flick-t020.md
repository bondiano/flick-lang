---
title: "FLICK-T020"
---

`unboxed type` and `untagged type` name how a variant type's values are
represented in JS, for code that meets them across the FFI. An unboxed type
has one constructor with one field and is that field. An untagged type is
the one field of each of its constructors, which JS must tell apart by
type: `Float`, `String`, `Bool`, `BigInt`, or one list or tuple; no two
constructors may share one.

```flick ignore
unboxed type UserId = UserId String

untagged type Key = ByNumber Float | ByName String
```

An enum is an untagged type whose constructors are literals, all strings
or all `Int`s and each different; its constructors have no fields and the
type no parameters. A literal comes after `=`, and only in an `untagged
type`.

```flick ignore
untagged type Method = Get = "GET" | Post = "POST"
```
