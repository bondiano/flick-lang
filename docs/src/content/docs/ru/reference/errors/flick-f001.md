---
title: "FLICK-F001"
source_hash: "8ddc1d587884db59"
---

Внешние метаданные должны соответствовать сигнатуре и режиму: `arity` подсчитывает аргументы JS и не превышает количество параметров, `arity 0` опускает первый аргумент `()`, `optional i` именует аргумент `Option`, `variadic` распространяет последний `List`, `nullable` создаёт результат `Option`, `cancel signal` предназначен для промиса, строка с `Async`, а `catch` — для режима, который может выбрасывать исключения. `foreign sync` и `foreign async` — слова before: без слова режима сигнатура указывает, как вызывается хост.

```flick
now_millis : Unit ->{} Float
foreign now_millis from global "Date.now"
  arity 0
```
