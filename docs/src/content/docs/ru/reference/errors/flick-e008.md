---
title: "FLICK-E008"
source_hash: "42a39e850d8c7595"
---

Проект, собранный с помощью `[build] effect = "none"`, компилируется в обычный JavaScript без Effect, поэтому в нём ничего не может выполняться во время выполнения: ни `task`, ни `scope`, ни вызовов во время выполнения. Сохраняйте код чистым или собирайте с помощью `effect = "effect"`.

```flick ignore
// flick.toml: [build] effect = "none"
main : Unit -> Unit
let main () = ()
```
