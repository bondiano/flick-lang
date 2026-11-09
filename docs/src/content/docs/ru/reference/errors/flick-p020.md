---
title: "FLICK-P020"
source_hash: "8ad615796487f69a"
---

`flick pack` проверяет, что пакет является полным перед созданием архива: каждый модуль имеет свою `.iface`, каждый объявленный фасад TypeScript существует, и каждый интерфейс зависимости имеет отпечаток, с которым были собраны модули.

```sh
flick build --profile release --emit-dts
flick pack --out-dir artifacts
```
