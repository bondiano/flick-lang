---
title: "FLICK-T074"
source_hash: "9821b8e8f52ccc12"
---

# Запись в фигурных скобках ветки `if`

Фигурные скобки сразу после `then` или `else` содержат инструкции, как
скобки после `do` и `loop`: `if c then { a = 1; b = 2 }` присваивает две
переменные. Запись на этом месте пишется в круглых скобках,
`then ({ r with done = true })`.

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

`flick check --fix --only T074` заключает в круглые скобки каждую запись,
написанную по-старому.
