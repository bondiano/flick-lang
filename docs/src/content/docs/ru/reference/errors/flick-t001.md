---
title: "FLICK-T001"
source_hash: "57b734357a357a54"
---

Имя, используемое в типе или выражении, не определено в модуле, не импортировано и не является встроенным. Квалифицированные имена требуют модуля, который импортируется файлом (`import App.Users`), встроенного модуля (`Ref`, `String`, …) или сервиса, объявленного в области видимости (`Users.get`). Переменные типа объявления `type` должны быть его параметрами.

```flick ignore
type alias Pair a b = (a, b)  // `b` is a parameter, so it may be used

total : List Float -> Float
let total items = List.sum items
```
