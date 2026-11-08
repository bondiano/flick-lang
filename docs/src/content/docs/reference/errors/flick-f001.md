---
title: "FLICK-F001"
---

Foreign metadata must fit the signature and the mode: `arity` counts the JS
arguments and is at most the number of parameters, `arity 0` drops a first
`()` argument, `optional i` names an `Option` argument, `variadic` spreads a
last `List`, `nullable` makes an `Option` result, `cancel signal` is for
a promise, a row with `Async`, and `catch` is for a mode that can throw.
`foreign sync` and `foreign async` are words of before: without a mode word
the signature tells how the host is called.

```flick ignore
now_millis : Unit ->{} Float
foreign now_millis from global "Date.now"
  arity 0
```
