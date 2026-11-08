---
title: "FLICK-P007"
source_hash: "c183aa31dcdf9ed1"
---

Проект с `kind = "app"` должен назвать свой входной модуль: модуль, который экспортирует `main : Unit ->{; AppError} Unit`. У библиотеки нет входного модуля.

```toml
[project]
kind = "app"
entry = "App.Main"
```
