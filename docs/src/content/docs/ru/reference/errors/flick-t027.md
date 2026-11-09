---
title: "FLICK-T027"
source_hash: "f282df4d1eb8c4a7"
---

Проект устанавливает `no-let-assert = true` в таблице `[lint]` из `flick.toml`: ни один `let assert` не может останавливать его программы. Обрабатывайте каждое значение с помощью `match`; исправление перемещает остальную часть блока в ветвь шаблона и добавляет ветвь `-> todo` для того, что пропущено.

```flick
module App.Config exposing (port)

port : List Float -> Float
let port ports =
  match ports with
  | [first, ..._] -> first
  | [] -> 8080
```
