---
title: "FLICK-S005"
---

A string literal contains an invalid escape or a malformed multi-line string.
Escapes are `\n \t \r \\ \" \0` and `\u{…}` with 1 to 6 hex digits naming a
Unicode scalar value (no surrogates). `\0` cannot be followed by a digit. A
multi-line string opens with `"""` followed by a line break. Raw strings
`r"…"` and `r#"…"#` have no escapes.

```flick
let path = r"C:\temp"
let smile = "\u{1F600}"
```
