---
title: "FLICK-U007"
source_hash: "47063ce4e38cbbdd"
---

Функция, которая читает атомы (`Atom.Read`) и возвращает `Ui`, рендерится как часть того, кто её вызывает: когда один из её атомов изменяется, весь вызывающий компонент рендерится заново. `component` рендерится самостоятельно, поэтому рендерится заново только он.

```flick
count : Atom.Writable Float
let count = Atom.make 0

component counter () = ui { text (show (Atom.value count)) }
```

Внутри `ui { … }` напишите `counter ()` там, где была вызвана функция.
