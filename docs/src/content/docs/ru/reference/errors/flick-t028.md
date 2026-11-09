---
title: "FLICK-T028"
source_hash: "d794180bc62ed1eb"
---

Предупреждение: ветка `else` является просто `()`, которой `if` без `else` уже является, поэтому её можно удалить без изменений. Быстрое исправление удаляет её; `flick check --fix` удаляет все. Отключите предупреждение с помощью `allow = ["FLICK-T028"]` под `[lint]` в `flick.toml`.

```flick
# verbose : Bool
# let verbose = false
# import Console
# warn : String ->{} Unit
let warn msg =
  if verbose then
    Console.log msg
```
