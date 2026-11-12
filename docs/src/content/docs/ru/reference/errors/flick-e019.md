---
title: "FLICK-E019"
source_hash: "6b4cdfa8337af1b8"
---

Обработчик находит сервис по имени, а не по аргументам. `provide` слоя
`Store Int` отвечает на каждую операцию `Store` в своём теле, поэтому
переменная ряда тела, которая может содержать `Store` с другими аргументами,
например `Store String`, получила бы вместо него `Store Int` из слоя.
Исключите сервис из переменной, `{-Store | s}`, чтобы вызывающий код не мог
передать задачу, которой он нужен, или назовите экземпляр, нужный телу,
`{Store Int | s}`.

```flick fail E019
service Store a =
  get : Unit ->{} a

answered : Layer none none {Store Int} -> Eff s none a ->{s} a
let answered layer t = provide layer { await t }
```

```flick
service Store a =
  get : Unit ->{} a

apart : Layer none none {Store Int} -> Eff {-Store | s} none a ->{s} a
let apart layer t = provide layer { await t }

named : Layer none none {Store Int} -> Eff {Store Int | s} none a ->{s} a
let named layer t = provide layer { await t }
```
