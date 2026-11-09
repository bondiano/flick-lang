---
title: "FLICK-T022"
source_hash: "9535f5540af328a6"
---

`todo` обозначает код, который ещё не написан. Он имеет тот тип, который требуется на его месте, чтобы остальные проверки модуля проходили, а ошибка возникала при выполнении.

`todo "what is missing"` сообщает, что именно он обозначает, в сообщении об ошибке.

```flick fail T022
module App.Report exposing (render)

render : List Float -> String
let render rows = todo "render the rows as a table"
```

`flick pack` отклоняет пакет, в котором остался `todo`.
