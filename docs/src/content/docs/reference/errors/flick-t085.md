---
title: "FLICK-T085"
---

# A statement after the children of a block

In a block whose lines are the children of one node, such as `ui { … }`,
the statements come first and the children after them: two or more
expression lines are the children of one `Ui.fragment`, and a `let` or an
assignment among them would split them.

```flick fail T085
view : Float -> Ui
let view n = ui {
  heading (text "count")
  let shown = show n
  paragraph (text shown)
}
```

is

```flick
view : Float -> Ui
let view n = ui {
  let shown = show n
  heading (text "count")
  paragraph (text shown)
}
```

`flick check --fix --only T085` moves such a `let` above the first child
when no line in between names what it binds or binds a name it uses.
