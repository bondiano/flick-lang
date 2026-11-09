---
title: "FLICK-P024"
source_hash: "a9c4e9f659802908"
---

`flick pack` отклоняет пакет, модули которого всё ещё содержат `todo` или `echo`:
`todo` завершается с ошибкой при выполнении, а `echo` пишет в stderr у пользователя
пакета. Каждое из них отображается как ошибка; исправьте или удалите её, затем упакуйте
снова.

```flick
render : List Float -> String
let render rows = todo "render the rows as a table"
```
