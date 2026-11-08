---
title: "FLICK-S011"
---

A metadata line under a `foreign` declaration has an unknown name. Known
metadata: `arity`, `this`, `catch`, `cancel`, `nullable`, `retention`,
`optional`, `variadic`.

```flick ignore
read_text : Path ->{Async; FsError} String
foreign read_text from "./fs.js" "readText"
  arity 1
  catch FsError.from_js
  cancel signal
```
