---
title: "FLICK-S006"
---

A numeric literal does not follow the JavaScript `NumericLiteral` grammar as
Flick restricts it: no leading zero (`010`; use `0o10`), `_` only between two
digits, digits after `0x`/`0o`/`0b` and in an exponent, BigInt `n` only on
integers, and no letters right after the number. Unlike JavaScript, `.5` and
`5.` are rejected because `.` is field access.

```flick ignore
let ratio = 0.5
let million = 1_000_000
let big = 9007199254740993n
```
