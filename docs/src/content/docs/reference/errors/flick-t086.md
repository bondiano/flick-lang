---
title: "FLICK-T086"
---

# The function of a block's lines written around them

In a block whose lines are the children of one node, such as `ui { … }`,
two or more lines are already the children of one `Ui.fragment`. A
`fragment` of children written one by one around them says the same.

```flick keep
view : Ui
let view = ui {
  fragment
    heading (text "count")
    paragraph (text "one")
}
```

is

```flick
view : Ui
let view = ui {
  heading (text "count")
  paragraph (text "one")
}
```

`flick check --fix --only T086` writes the children as the lines of the
block: lines under the call move to its column, and children on its line
each go on a line of their own, or after a `;` on the line of `ui {`. One
child, `fragment x`, and one list, `fragment rows`, are left as they are.
