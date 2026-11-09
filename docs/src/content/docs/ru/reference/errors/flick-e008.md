---
title: "FLICK-E008"
source_hash: "200634908b879afc"
---

Проект, собранный с помощью `[build] effect = "none"`, компилируется в обычный JavaScript без Effect, поэтому в нём ничего не может выполняться во время выполнения: ни `task`, ни `scope`, ни вызовов во время выполнения. Сохраняйте код чистым или собирайте с помощью `effect = "effect"`.

```flick
// flick.toml: [build] effect = "none"
main : Unit -> Unit
let main () = ()
```
