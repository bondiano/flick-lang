---
title: "FLICK-U001"
---

`Action.queue capacity` and `Ui.Event.queue capacity` keep at most
`capacity` events waiting, and `Action.concurrent limit` and
`Ui.Event.concurrent limit` run at most `limit` tasks at once; both are
positive integers. A constant that is not one is refused when the program
compiles; a value computed at run time is checked when the handler is made.

```flick ignore
on_click = Ui.Event.queue 8 (fun _ -> task { save () })
```
