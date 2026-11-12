---
title: "FLICK-S007"
---

The parser found a token where the grammar does not allow it. The message
names what was expected. After the error the parser skips to the next
statement or declaration, so later declarations are still checked.

```flick
# add : Float -> Float -> Float
// error: `let add left right = left +`
let add left right =
  left + right
```

`<|` and `|>` do not mix, nor `>>` and `<<`: either operator of
`x |> f <| y` could apply first, so parentheses say which.

```flick
# adder : Int -> Int -> Int
# let adder a b = a + b
// error: `let both = 1 |> adder <| 2`
both : Int
let both = (1 |> adder) <| 2
```
