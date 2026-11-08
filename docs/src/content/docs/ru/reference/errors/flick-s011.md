---
title: "FLICK-S011"
source_hash: "83a626965cdca0d7"
---

Строка метаданных под объявлением `foreign` содержит неизвестное имя. Известные метаданные: `arity`, `this`, `catch`, `cancel`, `nullable`, `retention`, `optional`, `variadic`.

```flick ignore
read_text : Path ->{Async; FsError} String
foreign read_text from "./fs.js" "readText"
  arity 1
  catch FsError.from_js
  cancel signal
```
