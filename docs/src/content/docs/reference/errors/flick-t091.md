---
title: "FLICK-T091"
---

Two applications of `symm` return to the original equation. In a checked
proof, you can use the inner proof directly. The quick fix is available
for a single-line step whose inner proof is a simple name or `refl`.

```flick
proof same_order (x : Bool) (y : Bool) (h : equal x y)
where equal x y = symm (symm h)
```

```flick
proof same_order (x : Bool) (y : Bool) (h : equal x y)
where equal x y = h
```
