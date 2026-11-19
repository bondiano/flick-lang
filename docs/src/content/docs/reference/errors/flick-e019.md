---
title: "FLICK-E019"
---

A handler finds a service or an effect by its name, not by its arguments.
`provide` of a layer or a record of `Store Int`, and a `try` with arms for
`Store Int`, answer every `Store` operation of their body, so a row variable
of the body that may hold `Store` at other arguments, `Store String` say,
would get the `Store Int` of the handler for it. Take the service out of the
variable, `{-Store | s}`, so a caller cannot pass work that needs it, or name
the instance the body needs, `{Store Int | s}`. A function of a row variable
given where one of `{Store Int | s}` is expected is reported the same way.

```flick fail E019
service Store a =
  get : Unit ->{} a

answered : Layer none none {Store Int} -> Eff s none a ->{s} a
let answered layer t = provide layer { await t }
```

```flick fail E019
effect Ask a =
  ask : Unit ->{} a

hidden : (Unit ->{r; e} Int) ->{r; e} Int
let hidden body =
  try body () with
  | Ask.ask () -> 41
```

```flick
service Store a =
  get : Unit ->{} a

apart : Layer none none {Store Int} -> Eff {-Store | s} none a ->{s} a
let apart layer t = provide layer { await t }

named : Layer none none {Store Int} -> Eff {Store Int | s} none a ->{s} a
let named layer t = provide layer { await t }
```
