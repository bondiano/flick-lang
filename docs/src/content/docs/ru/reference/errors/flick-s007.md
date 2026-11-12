---
title: "FLICK-S007"
source_hash: "fa3fa3f8ee8ea833"
---

Парсер обнаружил токен в месте, где грамматика его не допускает. В сообщении указано, что ожидалось. После ошибки парсер пропускает до следующего оператора или объявления, поэтому последующие объявления всё ещё проверяются.

```flick
# add : Float -> Float -> Float
// error: `let add left right = left +`
let add left right =
  left + right
```

`<|` и `|>` не смешиваются, как и `>>` с `<<`: в `x |> f <| y` первым
может примениться любой из операторов, поэтому порядок задают скобки.

```flick
# adder : Int -> Int -> Int
# let adder a b = a + b
// error: `let both = 1 |> adder <| 2`
both : Int
let both = (1 |> adder) <| 2
```
