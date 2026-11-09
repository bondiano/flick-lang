---
title: "FLICK-T003"
---

An expression has a type other than the one its position requires. Flick has no
implicit conversions: `1 + 1n` mixes `Float` and `BigInt`, `"a" ++ 1` mixes
`String` and `Float`. Records are exact: a record with extra fields does not
pass for a smaller closed record type (no width subtyping); accept the extra
fields with an open row `{ name : String | fields }` or project them away.

Types produced by different named functor applications are also distinct,
even if both are displayed as `Set` in source. The diagnostic shows their full
module paths. Import the same result module in both files, or convert values
explicitly through operations exposed by the two modules (for example,
`ExactSet.from_list (CaseInsensitiveSet.to_list names)`).

```flick
label : Float -> String
let label n = "count: " ++ Float.to_string n
```
