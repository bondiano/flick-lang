---
title: "FLICK-T076"
source_hash: "bc53a009b5817b26"
---

`T.prove v` создаёт значение типа с инвариантом, например refined type, без проверки во время выполнения. Компилятор должен доказать, что проверка типа выполняется для `v`, из того, что устанавливает путь к этому месту: условие охватывающего `if`, `T.check v`, вернувший `true`, или инвариант `S.value s`. Здесь этого ничего не устанавливает, поэтому программа отвергается. Значение вполне может пройти проверку; компилятор просто не может этого знать. Сначала проверьте его или используйте `T.parse v`, который проверяет значение во время выполнения и возвращает `Result`.

```flick fail T076
refined type Email = String where Checks.email

as_email : String -> Email
let as_email text = Email.prove text
```

это

```flick
refined type Email = String where Checks.email

as_email : String -> Option Email
let as_email text =
  if Email.check text then Some (Email.prove text) else None
```
