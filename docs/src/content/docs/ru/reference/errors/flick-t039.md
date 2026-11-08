---
title: "FLICK-T039"
source_hash: "a03eccc9df7f47ed"
---

Функция принимает больше параметров, чем `[lint] max-params` в `flick.toml`
(по умолчанию 5). Вызов с таким количеством позиционных аргументов легко сделать
в неправильном порядке. Используйте запись: её поля именованы при каждом вызове.

```text
type alias Box = { x : Float, y : Float, width : Float, height : Float, label : String, color : String }

draw : Box -> Ui
let draw box = Ui.text box.label
```

Повышайте лимит с помощью `max-params = 7` в `[lint]` или отключайте предупреждение
с помощью `allow = ["FLICK-T039"]`.
