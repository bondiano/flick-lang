---
title: "FLICK-P023"
source_hash: "19e3e024ea495dcd"
---

`[sql] dialect` в `flick.toml` не является названием диалекта SQL. Названиями являются `postgresql`, `mysql`, `sqlite`, `clickhouse` и `oracle`; в настоящее время поддерживается только `postgresql`.

```toml
[sql]
dialect = "postgresql"
```
