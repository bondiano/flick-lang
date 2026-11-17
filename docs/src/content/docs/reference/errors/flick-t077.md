---
title: "FLICK-T077"
---

`T.prove v` makes a value of a type with an invariant, such as a refined
type, without checking it when the program runs. The compiler must prove
that the type's check holds for `v` from what the path to it establishes:
the condition of an `if` it is in, a `T.check v` that gave `true`, or the
invariant of a `S.value s`. Nothing here established it, so the program is
rejected. The value may well pass the check; the compiler cannot tell.
Check it first, or use `T.parse v`, which checks it when the program runs
and gives a `Result`.

```flick fail T077
refined type Email = String where Checks.email

as_email : String -> Email
let as_email text = Email.prove text
```

is

```flick
refined type Email = String where Checks.email

as_email : String -> Option Email
let as_email text =
  if Email.check text then Some (Email.prove text) else None
```
