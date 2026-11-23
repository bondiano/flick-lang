---
title: "FLICK-F001"
---

Foreign metadata must fit the signature, the mode, and the active backend's
adapter rules: `arity` counts the host arguments and is at most the number of
parameters, `arity 0` drops a first `()` argument, `optional i` names an
`Option` argument, `owned i` names a parameter index within `arity`, `nullable`
makes an `Option` result, `catch` is for a mode that can throw, and backend
adapter words (such as `this`, `variadic`, and `cancel signal` on `flick/js`
and `flick/go`) must match their declared argument shapes and target checks.
`foreign sync` and `foreign async` are words of before: without a mode word
the signature tells how the host is called.

```flick
now_millis : Unit ->{} Float
foreign now_millis from global "Date.now"
  arity 0
```
