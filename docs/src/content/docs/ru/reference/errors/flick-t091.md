---
title: "FLICK-T091"
source_hash: "3a0fee5344c7ed08"
---

Два применения `symm` возвращают исходное равенство. В проверенном доказательстве можно сразу использовать внутреннее доказательство. Быстрое исправление доступно для однострочного шага, в котором внутреннее доказательство записано простым именем или `refl`.

```flick
proof same_order (x : Bool) (y : Bool) (h : equal x y)
where equal x y = symm (symm h)
```

```flick
proof same_order (x : Bool) (y : Bool) (h : equal x y)
where equal x y = h
```
