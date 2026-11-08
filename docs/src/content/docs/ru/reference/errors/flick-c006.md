---
title: "FLICK-C006"
source_hash: "83d3db785ab8a69f"
---

Только пакет класса или пакет номинального типа в начале первого типа экземпляра может объявлять экземпляр. Другой пакет оборачивает тип в собственный номинальный тип; прозрачный алиас таковым не является.

```flick ignore
type LocalExternalId = LocalExternalId ExternalId

instance Hash LocalExternalId =
  hash : LocalExternalId -> Float
  let hash id =
    match id with
    | LocalExternalId raw -> hash_external raw
```
