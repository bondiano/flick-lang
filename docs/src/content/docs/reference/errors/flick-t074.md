---
title: "FLICK-T074"
---

# A record in the braces of an `if` branch

Braces right after `then` or `else` hold statements, as the braces after
`do` and `loop` do: `if c then { a = 1; b = 2 }` assigns two variables. A
record there goes in parentheses, `then ({ r with done = true })`.

```flick
module App.Order exposing (ordered, finish)

ordered : Int -> Int -> (Int, Int)
let ordered a b =
  let mut low = a
  let mut high = b
  if low > high then {
    low = b
    high = a
  }
  (low, high)

type alias Item = { title : String; done : Bool }

finish : Item -> Item
let finish item = if item.done then item else ({ item with done = true })
```

`flick check --fix --only T074` puts each record written the old way in
parentheses.
