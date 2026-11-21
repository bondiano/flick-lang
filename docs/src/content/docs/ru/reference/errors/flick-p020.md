---
title: "FLICK-P020"
source_hash: "94f950a0cdff7b01"
---

`flick pack` проверяет, что пакет является полным перед созданием архива: каждый модуль имеет свою `.iface`, каждый объявленный фасад TypeScript существует, и каждый интерфейс зависимости имеет отпечаток, с которым были собраны модули.

```sh
flick build --profile release --declarations
flick pack --out-dir artifacts
```
