---
title: "FLICK-T052"
source_hash: "ad75e2382eaf456a"
---

`fun x -> f x` — это функция, которая вызывает `f` с её параметром: `f` — это уже та функция. Быстрое исправление записывает `f`. При большем количестве аргументов `fun x -> f a x` — это `f a`. Это исправление не применяется `flick check --fix`, потому что `f a` тогда выполняется один раз при создании функции. Метод класса, такой как `pure`, не отмечается: он находит свой экземпляр при вызове.

```flick
# double : Float -> Float
# let double x = x * 2
doubled : List Float -> List Float
let doubled xs = List.map double xs
```
