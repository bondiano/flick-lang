---
title: "FLICK-R009"
source_hash: "a67929f082737aa1"
---

Определение верхнего уровня имеет имя, которое модуль импортирует по имени в
`exposing (…)`. Использование этого имени будет тихо означать локальное. Переименуйте
определение или удалите имя из списка `exposing` импорта и используйте квалификацию.

```flick fail R009
import App.Text exposing (trim)

trim : String -> String
let trim s = String.trim s
```
