---
title: "FLICK-R003"
source_hash: "0d9c923cd367cb3f"
---

Ни один файл и ни один установленный пакет не предоставляют импортируемый модуль. Локальный модуль
`App.Users` находится в `src/App/Users.fl`; тестовый модуль также может импортировать из
корня тестов. Модуль из npm-пакета требует наличия пакета в `package.json` и его установки: выполните `flick add <package>`, затем `flick install`.

```flick
import App.Users exposing (load_user)   // src/App/Users.fl
```
