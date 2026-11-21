---
title: "FLICK-P057"
source_hash: "b8ea5ccefb13d4f4"
---

Этот флаг CLI или ключ `[build]` принадлежал одному конкретному бэкенду и был
удалён, когда опции бэкендов были вынесены из ядра CLI. Алиасов совместимости
нет: замените его на `--target <name>` и собственные опции выбранного бэкенда
или таблицу `[target.<name>]`.

```toml
[build]
target = "js"

[target.js]
typescript = true
declarations = true
runtime = "rt"
```
