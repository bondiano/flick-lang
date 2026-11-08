---
title: "FLICK-T005"
source_hash: "2366717b7ad3a56c"
---

Flick записывает типы в сигнатуре над функцией, а не в её `let` заголовке:  
в заголовке сохраняются только имена аргументов. Переместите типы в сигнатуру.

```flick ignore
// not: let area (w : Float) (h : Float) : Float = w * h
area : Float -> Float -> Float
let area w h = w * h
```
