---
title: "FLICK-R009"
source_hash: "929749d156ac4c87"
---

Определение верхнего уровня имеет имя, которое модуль импортирует по имени в
`exposing (…)`. Использование этого имени будет тихо означать локальное. Переименуйте
определение или удалите имя из списка `exposing` импорта и используйте квалификацию.

```flick ignore
import App.Text as Text

trim : String -> String
let trim s = Text.trim s
```
