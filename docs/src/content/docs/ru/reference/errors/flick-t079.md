---
title: "FLICK-T079"
source_hash: "1b871d52dcb1c510"
---

`T.prove` создаёт значение типа с инвариантом только применённым к своему аргументу, `T.prove v`, где компилятор доказывает, что проверка типа выполняется для этого `v`. Это не функция: названный отдельно, переданный другой функции или сохранённый в списке или значении, он создавал бы значения, которые ничто не доказало. Применяйте его там, где известны факты о его аргументе.

```flick fail T079
refined type Email = String where Checks.email

make : String -> Email
let make = Email.prove
```

это

```flick
refined type Email = String where Checks.email

make : String -> Option Email
let make text = if Email.check text then Some (Email.prove text) else None
```
