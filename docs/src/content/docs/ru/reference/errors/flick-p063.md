---
title: "FLICK-P063"
source_hash: "1fcdcae3001b1194"
---

Ядро управляет библиотеками Flick из `[dependencies]`: `flick add` с
`--path`, `--git` или `--url` добавляет такую библиотеку. Все остальные
пакеты относятся к экосистеме бэкенда проекта, который реализует для них
собственные команды `install`, `add`, `remove`, `update` и `deps`. У бэкенда
этого проекта нет такой команды — или она не принимает эту опцию, — поэтому
`flick` некуда передать пакет.

Укажите библиотеку Flick с опцией источника или соберите проект с бэкендом,
в экосистеме которого есть этот пакет.

```sh
flick add geometry --git https://example.com/geometry.git --tag v1.2.0
flick add geometry --path ../geometry
flick add geometry --url https://example.com/geometry-1.2.0.tgz --sha256 <hash>
```
