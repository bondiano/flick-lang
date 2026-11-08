---
title: "FLICK-U001"
source_hash: "61998612146dfac3"
---

`Action.queue capacity` и `Ui.Event.queue capacity` удерживают в ожидании не более
`capacity` событий, а `Action.concurrent limit` и
`Ui.Event.concurrent limit` запускают одновременно не более `limit` задач; оба значения — положительные целые числа. Константа, отличная от единицы, отклоняется при компиляции программы; значение, вычисленное во время выполнения, проверяется при создании обработчика.

```flick ignore
on_click = Ui.Event.queue 8 (fun _ -> task { save () })
```
