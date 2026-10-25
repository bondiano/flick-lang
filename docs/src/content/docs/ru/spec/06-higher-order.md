---
title: "6. Функции высшего порядка и обход коллекций"
---

## Чистый и эффектный обход

```flick
normalize_names : List String -> List String
let normalize_names raw_names =
  List.map normalize_name raw_names

load_users : List UserId ->{Users; NotFound} List User
let load_users user_ids =
  List.map_effect Users.get user_ids

requests_for : List UserId -> List (Eff {Users} {NotFound} User)
let requests_for user_ids =
  List.map
    (fun id -> task { Users.get id })
    user_ids

load_users_parallel : List UserId ->{Users; NotFound} List User
let load_users_parallel user_ids =
  parallel_all 8 (requests_for user_ids)
```

В первой версии `List.map` принимает чистую функцию, `List.map_effect` выполняет переданную эффектную функцию последовательно слева направо, а `parallel_all` требует явный предел конкурентности. Построение `requests` не запускает запросы. Единого `map`, полиморфного по эффектам, нет.

```text
List.map :
  forall c a b.
  (a -> b)^c -> (List a -> List b)^c

List.map_effect :
  forall c a b r e.
  (a ->{r; e} b)^c ->
  (List a ->{r; e} List b)^c
```

Частичное применение удерживает переданную функцию и её захваты `c`. После немедленного обхода результирующий `List b` сохраняет захваты самого `b`, но не обязан сохранять уже выполненную функцию. Это точнее общего `Functor.map`, который допускает отложенное применение.

Альтернатива — полиморфный по эффектам `map`, у которого эффект зависит от переданной функции. Тогда нужно выбрать соглашение о вызовах для случаев, когда она возвращает обычное JS-значение или Effect: специализацию, единое представление либо дополнительную абстракцию. Пример такой типизации есть в [Koka](https://koka-lang.github.io/koka/doc/book.html).

## `do`, `for` и списковые выражения

```flick
pair_optional : Option a -> Option b -> Option (a, b)
let pair_optional left right =
  do {
    x <- left
    y <- right
    pure (x, y)
  }
```

`do` — сахар над `Monad` из прелюдии: `p <- e` связывает через `flat_map`, голое выражение выполняется и отбрасывается, `if c` — фильтр через `Alternative`, последнее выражение — результат. Продолжения — чистые функции, как требует `flat_map`: эффектный вызов внутри `do` над `Option` — ошибка с подсказкой связать его до блока. Опровержимый образец после `<-` — ошибка; для него есть `match`. `do` над `Eff` разрешён, но линт предлагает исправление в прямые вызовы, которые объединяют строки, зафиксированные в `Monad (Eff r e)`.

`for { x <- xs; if p x; y <- ys x } yield e` — ленивый `Seq`, который компилируется в один нативный генератор; `[for … yield e]` — строгий `List`, который компилируется в цикл. Внутри `for` опровержимый образец после `<-` пропускает неподходящие элементы. Тело генератора чистое; `Seq` сохраняет захваты того, что использует, потому что выполняется позже.

Цена — второй стиль рядом с прямыми вызовами; линты удерживают эффектный код на прямых вызовах. Выигрыш — короткий код для `Option`, `Result`, списков, парсеров и пользовательских монад.
