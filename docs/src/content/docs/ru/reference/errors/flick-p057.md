---
title: "FLICK-P057"
source_hash: "f4522c57fd1583b3"
---

Этот флаг CLI, ключ `[build]` или таблица верхнего уровня для конкретного
бэкенда (например, `[js]` и `[js.dependencies]`) принадлежали одному бэкенду и
были удалены, когда настройки бэкендов вынесли из ядра CLI. Алиасов
совместимости нет: замените их на `--target <name>` и собственные опции
выбранного бэкенда или таблицы `[target.<name>]` и
`[target.<name>.dependencies]`.

```toml
[build]
target = "js"

[target.js]
typescript = true
declarations = true
runtime = "rt"

[target.js.dependencies]
left-pad = "^1.3.0"
```
