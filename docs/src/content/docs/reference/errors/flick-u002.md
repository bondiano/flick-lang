---
title: "FLICK-U002"
---

A handler made from a `fun` inside the render of `Ui.view` is a new
closure every time the view renders. The listener React holds stays the
same, but the closure is allocated again and props holding it never compare
equal. Declare the handler once as an `Action` at the top level and pass
what differs between elements as data with `Action.with`.

```flick
# type alias Item = { name : String }
# items : Atom.Writable (List Item)
# let items = Atom.make []
remove : Action.Action (Item, Ui.ClickSnapshot)
let remove = Action.exhaust (fun (item, _) -> task { Atom.update items (List.filter (fun i -> i != item)) })

item_view : Ui.View Item
let item_view = Ui.view (fun item -> ui { button { disabled = false; on_click = Action.with remove item } [text item.name] })
```
