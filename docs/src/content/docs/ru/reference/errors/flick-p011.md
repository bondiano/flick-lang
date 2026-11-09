---
title: "FLICK-P011"
source_hash: "caaf6b16766ac730"
---

`entry` приложения должен указывать на модуль в корне исходного кода, который экспортирует `main`.

```flick
module App.Main exposing (main)

main : Unit ->{; AppError} Unit
let main () = ()
```
