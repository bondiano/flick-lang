---
title: "FLICK-P022"
source_hash: "b5e80aeb7775784f"
---

`[sql] dialect` в `flick.toml` обозначает диалект SQL, известный Flick, но пока не проверяемый: `mysql`, `sqlite`, `clickhouse` или `oracle`. Поддерживается только `postgresql`; он также является значением по умолчанию.

```toml
[sql]
dialect = "postgresql"
```
