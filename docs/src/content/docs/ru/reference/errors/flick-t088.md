---
title: "FLICK-T088"
source_hash: "82adf8c0df5c733a"
---

Когда функция вызывает другую функцию, контракт которой уже проверен,
`flick prove` проверяет, что условия пути вызывающей функции устанавливают
каждое предусловие `require` вызываемой функции в точке вызова. Усильте
`require` вызывающей функции или защитите вызов проверкой.

```flick fail T088
consume : Int -> Int -> Int
let consume available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining && remaining + used == available)
=
  available - used

take : Int -> Int -> Int
let take available used
where
  require (0 <= used)
  ensure (fun remaining -> 0 <= remaining)
=
  consume available used
```

это

```flick
consume : Int -> Int -> Int
let consume available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining && remaining + used == available)
=
  available - used

take : Int -> Int -> Int
let take available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining)
=
  consume available used
```
