---
title: "FLICK-T019"
source_hash: "486ddc1d9a6e0644"
---

Последним выражением блока является его значение, поэтому блок не может заканчиваться на `let`
или сигнатуру. Добавьте выражение, к которому сводится блок.

```flick ignore
double : Float -> Float
let double n =
  let twice = n * 2
  twice
```
