---
title: "FLICK-S004"
---

A string literal or block comment is not closed before the end of the line
(ordinary strings) or of the file (multi-line and raw strings, `/* */`).
Ordinary strings cannot span lines: use `\n` or a `"""` string.

```flick
let greeting = "hello"
let poem = """
  first line
  second line
  """
```
