---
title: "FLICK-T005"
source_hash: "657c3cf16a5b678c"
---

Flick записывает типы в сигнатуре над функцией, а не в её `let` заголовке:  
в заголовке сохраняются только имена аргументов. Переместите типы в сигнатуру.

```flick
// not: let area (w : Float) (h : Float) : Float = w * h
area : Float -> Float -> Float
let area w h = w * h
```
