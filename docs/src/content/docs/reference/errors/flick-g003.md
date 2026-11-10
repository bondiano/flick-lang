---
title: "FLICK-G003"
---

`Fiber.spawn owner supervisor task` registers the task with `owner`: it runs
until the owner's scope closes. The task and its supervisor may use only
regions that end no earlier, so a task that uses a resource of an inner scope
cannot be given to the owner of an outer one. Spawn it with the inner owner.

```text
scope outer {
  scope inner {
    let file = File.open_scoped path
    // was: Fiber.spawn outer report (task { File.read_all file })
    Fiber.spawn inner report (task { File.read_all file })
  }
}
```
