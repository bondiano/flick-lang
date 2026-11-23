---
title: "FLICK-P020"
source_hash: "72421d06ef574be7"
---

`flick pack` проверяет полноту релизных артефактов перед записью пакетов
целевой экосистемы: каждый сгенерированный файл `.iface` должен читаться и
совпадать по отпечаткам с интерфейсами зависимостей, а необязательная команда
`pack` выбранного бэкенда проверяет собственные требования (например, для
`flick/js` — наличие компаньонов `.iface` и `.d.ts`, корректный `package.json`
с существующими путями `exports` и `[target.js] typescript = false` для
npm-пакетов).

```sh
flick build --profile release --declarations
flick pack --out-dir artifacts
```
