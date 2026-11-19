---
title: "FLICK-P057"
source_hash: "780fbb5f9dc5dcbe"
---

Ядро управляет библиотеками Flick из `[dependencies]`: `flick add` с `--git`, `--url` или `--path` добавляет такую библиотеку. Все остальные пакеты относятся к экосистеме бэкенда проекта, и бэкенд реализует для них `add`, `remove`, `update`, `install` и `deps` своими командами. У бэкенда этого проекта такой команды нет или она не принимает эту опцию, поэтому `flick` некуда передать пакет.

Укажите библиотеку Flick с опцией источника или соберите проект бэкендом, в экосистеме которого есть этот пакет.

```sh
flick add geometry --git https://example.com/geometry.git --tag v1.2.0
flick add geometry --path ../geometry
```
