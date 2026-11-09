---
title: "FLICK-P005"
source_hash: "4d838325cbe619eb"
---

`flick.toml` недопустим: ошибка синтаксиса TOML, ключ, неизвестный схеме,
или значение неправильного типа, например `kind`, отличное от `app` или `lib`.

```toml
schema-version = 1

[project]
kind = "app"
entry = "App.Main"
```
