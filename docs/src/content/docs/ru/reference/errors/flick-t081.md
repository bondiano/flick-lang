---
title: "FLICK-T081"
source_hash: "056f17d4d908462b"
---

`proof` утверждает, что две стороны равны. Правило в этом месте не делает их равными: после вычисления стороны всё ещё различаются, и сообщение показывает, что осталось. Утверждение может быть верным, но ему нужно другое доказательство: `match`, разбирающий переменную, чтобы каждый случай вычислялся дальше, `funext` для функций или предпосылка. Ниже `match` в цели ждёт `v`, и разбор `v` закрывает каждый случай.

```flick fail T081
proof rebuilt (v : Option a)
where equal (match v with | None -> None | Some x -> Some x) v =
  refl
```

это

```flick
proof rebuilt (v : Option a)
where equal (match v with | None -> None | Some x -> Some x) v =
  match v with
  | None -> refl
  | Some y -> refl
```
