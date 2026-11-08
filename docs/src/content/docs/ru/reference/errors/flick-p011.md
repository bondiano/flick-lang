---
title: "FLICK-P011"
source_hash: "a392a1aeb9dc37fa"
---

`entry` приложения должен указывать на модуль в корне исходного кода, который экспортирует `main`.

```flick ignore
module App.Main exposing (main)

main : Unit ->{; AppError} Unit
let main () = ()
```
