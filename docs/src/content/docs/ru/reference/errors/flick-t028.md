---
title: "FLICK-T028"
source_hash: "0b3027091673ea7f"
---

Предупреждение: ветка `else` является просто `()`, которой `if` без `else` уже является, поэтому её можно удалить без изменений. Быстрое исправление удаляет её; `flick check --fix` удаляет все. Отключите предупреждение с помощью `allow = ["FLICK-T028"]` под `[lint]` в `flick.toml`.

```flick ignore
let warn msg =
  if verbose then
    Console.log msg
```
