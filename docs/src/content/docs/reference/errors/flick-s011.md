---
title: "FLICK-S011"
---

A metadata line under a `foreign` declaration has a name that is neither a core
foreign contract word (`arity`, `catch`, `retention`, `nullable`, `optional`,
`reviewed`, `owned`) nor an adapter word declared by the active package's
backend (for example `this`, `cancel`, `variadic` on `flick/js` and `flick/go`).

```flick ignore
read_text : Path ->{Async; FsError} String
foreign read_text from "./fs.js" "readText"
  arity 1
  catch FsError.from_js
  cancel signal
```
