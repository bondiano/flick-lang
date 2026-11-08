---
title: "FLICK-P034"
source_hash: "91be87d6e85a603e"
---

Зависимость `git` или `url` отсутствует в `flick.lock`, либо её исходники не находятся в хранилище пакетов: либо они ещё не были загружены, либо с тех пор изменился манифест. `flick install` загружает то, что фиксирует `flick.lock`, и добавляет то, чего не хватает; `flick install --locked` завершает работу с ошибкой вместо изменения блокировки.

```toml
[dependencies]
text = { git = "https://github.com/acme/text", tag = "v1.2.0" }
```
