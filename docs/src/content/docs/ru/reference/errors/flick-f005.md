---
title: "FLICK-F005"
source_hash: "81ae587216258b9c"
---

`flick ffi import` записывает `foreign blocked name "reason"` для экспорта
пакета, который он не может безопасно связать: тип вне поддерживаемого подмножества или
контракт, который `.d.ts` не указывает (отклонения обещания, удержание обратного вызова,
nullable результат). Другие экспорты пакета остаются
используемыми; этот требует ручного написания и проверки связывания или политики
в `policy.toml` рядом с сгенерированным модулем и нового `ffi import`.

```flick ignore
// Generated.fl: foreign blocked lookup "needs policy: …"
lookup : String ->{Async; JsError} Option String
foreign lookup from "lookup-lib" "lookup"
  arity 1
  catch JsError.from_js
  nullable
  reviewed "lookup-lib@1.0.0 sha256:…"
```
