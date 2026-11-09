---
title: "FLICK-C006"
---

Only the package of the class, or the package of the nominal type at the
head of the instance's first type, may declare an instance. Another package
wraps the type in a nominal type of its own; a transparent alias is not one.

```flick ignore
type LocalExternalId = LocalExternalId ExternalId

instance Hash LocalExternalId =
  hash : LocalExternalId -> Float
  let hash id =
    match id with
    | LocalExternalId raw -> hash_external raw
```
