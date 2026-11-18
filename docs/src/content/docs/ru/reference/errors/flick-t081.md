---
title: "FLICK-T081"
source_hash: "f04e6e96c01e23eb"
---

`proof` утверждает, что две стороны равны. Правило в этом месте не делает их равными: после вычисления стороны всё ещё различаются, и сообщение показывает, что осталось. Утверждение может быть верным, но ему нужно другое доказательство. Разберите переменную через `match`, чтобы каждый случай вычислялся дальше, или используйте `funext` для функций.

```flick fail T081
proof constant (f : Bool -> Bool)
where equal (f true) (f false) =
  refl
```

это

```flick
proof applied (f : Bool -> Bool) (x : Bool)
where equal ((fun y -> f y) x) (f x) =
  refl
```
