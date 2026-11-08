---
title: "FLICK-T037"
source_hash: "f04aa64b77d64bb6"
---

`if`, `match`, `fun`, `do`, `task`, `for` или `try` вложены глубже, чем `[lint] max-nesting` в `flick.toml` (по умолчанию 4). `else if` не поддерживает вложенность. Переместите внутреннюю часть в отдельную функцию или выполните сопоставление сразу по нескольким значениям.

```flick ignore
label : Option User -> String
let label user = match user with
  | Some u -> named u
  | None -> "anonymous"
```

Увеличьте лимит с помощью `max-nesting = 6` в разделе `[lint]` или отключите предупреждение с помощью `allow = ["FLICK-T037"]`.
