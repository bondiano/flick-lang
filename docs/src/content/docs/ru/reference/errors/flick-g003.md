---
title: "FLICK-G003"
source_hash: "45905e3aa48e5e9c"
---

`Fiber.spawn owner supervisor task` регистрирует задачу с `owner`: она выполняется до закрытия области владельца. Задача и её супервизор могут использовать только регионы, которые заканчиваются не раньше, поэтому задачу, использующую ресурс внутренней области, нельзя передавать владельцу внешней области. Запускайте её с внутренним владельцем.

```text
scope outer {
  scope inner {
    let file = File.open_scoped path
    // was: Fiber.spawn outer report (task { File.read_all file })
    Fiber.spawn inner report (task { File.read_all file })
  }
}
```
