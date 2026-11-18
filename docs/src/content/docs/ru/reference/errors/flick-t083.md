---
title: "FLICK-T083"
source_hash: "7f037ab139872570"
---

Тело `proof` — это сертификат: `refl`, правило с аргументами, `match`, разбирающий переменную утверждения на все её случаи, предпосылка или `let`, называющий доказательство. Здесь тело ничем из этого не является, или правило применено там, где оно не подходит: разбор пропускает случай, или `cong` получает доказательство, равенство которого нигде не записано.

```flick fail T083
proof wrapped (x : Bool) (y : Bool)
where equal (Some x) (Some y) =
  cong (fun z -> Some z) refl
```

это

```flick
proof wrapped (x : Bool) (y : Bool) (h : equal x y)
where equal (Some x) (Some y) =
  cong (fun z -> Some z) h
```
