---
title: "FLICK-T017"
source_hash: "6f3c5d518d67e320"
---

Рекурсия является явной: функция, которая вызывает сама себя, определяется с помощью `let rec`,  
а функции, которые вызывают друг друга, определяются вместе в одной группе `rec { … }`,  
каждая со своей собственной сигнатурой.

```flick
rec {
  is_even : Float -> Bool
  let is_even n = if n == 0 then true else is_odd (n - 1)

  is_odd : Float -> Bool
  let is_odd n = if n == 0 then false else is_even (n - 1)
}
```
