---
title: "FLICK-T073"
---

# Children of a call written as a list

Inside a block that gathers children, such as `ui { … }`, a function whose
last parameter is a list of them takes that list element by element: each
child is an argument of the call, on its line in parentheses or on a line of
its own under it, and a call with none has no children. A list literal there
would be one child that is a list.

```flick
view : Ui
let view = ui {
  div { class_name = "counter" }
    heading (text "count")
    paragraph
      text "one line"
      text "for each child"
    div { class_name = "spacer" }
}
```

`flick check --fix --only T073` rewrites each list written the old way. A
list that is computed is a node of its own: `Ui.each items row`.
