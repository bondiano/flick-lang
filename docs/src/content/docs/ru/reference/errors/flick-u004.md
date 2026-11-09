---
title: "FLICK-U004"
source_hash: "06e3076fd887d0b6"
---

Рендер представления (`Ui.view`) и вычисление производного атома (`Atom.derived`) используют только сервисы `Atom.Read`: React может повторно отрендерить представление в любое время, а реестр вычисляет производный атом всякий раз, когда изменяется атом, который он читает, поэтому они только читают атомы. Запись (`Atom.set`, `Atom.update`, требующая `Atom.Write`) должна происходить в `Action`, который запускается по событию; задача с другими сервисами выполняется как `Atom.task`, результат которой читает рендер; ошибка обрабатывается в рендере или представление создаётся с помощью `Ui.suspended`, который даёт каждой ошибке своё представление.

```flick ignore
count : Atom.Writable Float
let count = Atom.make 0

increment : Action.Action Ui.ClickSnapshot
let increment = Action.exhaust (fun _ -> task { Atom.update count (fun n -> n + 1) })

counter : Ui.View Unit
let counter =
  Ui.view (fun () -> ui { button { disabled = false; on_click = increment } [text (show (Atom.value count))] })
```
