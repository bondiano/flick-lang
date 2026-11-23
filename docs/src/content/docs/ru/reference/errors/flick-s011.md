---
title: "FLICK-S011"
source_hash: "0e474b5630c37333"
---

Строка метаданных под объявлением `foreign` содержит имя, которое не является
ни словом ядра контракта, ни словом адаптера, объявленным активным бэкендом
пакета.

Слова ядра контракта (доступны на каждом бэкенде): `arity`, `catch`,
`retention`, `nullable`, `optional`, `reviewed`, `owned`. Официальные бэкенды
дополнительно объявляют свои слова адаптеров в дескрипторе (например, `this`,
`cancel`, `variadic` в `flick/js` и `flick/go`).

```flick ignore
read_text : Path ->{Async; FsError} String
foreign read_text from "./fs.js" "readText"
  arity 1
  catch FsError.from_js
  cancel signal
```
