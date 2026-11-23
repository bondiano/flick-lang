---
title: "FLICK-S011"
source_hash: "fcc85164a15e1674"
---

Строка метаданных под объявлением `foreign` содержит имя, которое не является ни словом контракта ядра (`arity`, `catch`, `retention`, `nullable`, `optional`, `reviewed`, `owned`), ни словом адаптера, объявленным бэкендом активного пакета (например, `this`, `cancel`, `variadic` для `flick/js` и `flick/go`).

```flick ignore
read_text : Path ->{Async; FsError} String
foreign read_text from "./fs.js" "readText"
  arity 1
  catch FsError.from_js
  cancel signal
```
