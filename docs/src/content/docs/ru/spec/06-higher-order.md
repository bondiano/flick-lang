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

В первой версии `List.map` принимает чистый callback, `List.map_effect` выполняет runtime-callback последовательно слева направо, а `parallel_all` требует явный предел конкурентности. Построение `requests` не запускает запросы. Единого effect-polymorphic `map` нет.

```text
List.map :
  forall c a b.
  (a -> b)^c -> (List a -> List b)^c

List.map_effect :
  forall c a b r e.
  (a ->{r; e} b)^c ->
  (List a ->{r; e} List b)^c
```

Частичное применение удерживает callback и его `c`. После eager обхода результирующий `List b` сохраняет captures самого `b`, но не обязан сохранять уже выполненный callback. Это точнее общего `Functor.map`, который допускает отложенное применение.

Альтернатива — эффектно-полиморфный `map`, у которого эффект зависит от callback. Тогда нужно выбрать соглашение о вызовах для случаев, когда callback возвращает обычное JS-значение или Effect: специализацию, единое представление либо дополнительную абстракцию. Пример такой типизации есть в [Koka](https://koka-lang.github.io/koka/doc/book.html).

## Работа с Monad без специального do

```flick
pair_optional : Option a -> Option b -> Option (a, b)
let pair_optional left right =
  match left with
  | None -> None
  | Some x ->
    match right with
    | None -> None
    | Some y -> Some (x, y)
```

То же выражается обычными `Option.bind` и `Option.map`. Это чистая работа с данными; Effect runtime ей не требуется. HKT и классы типов доступны без generic `do`.

Generic `do` и `let*` в первую версию не входят. Прямые runtime-вызовы используют обычный `let`, холодные описания — `task`, запуск сохранённого описания в текущем fiber — `await`. Для пользовательских монад остаются обычные combinators.

Цена решения — более явный код для парсеров и пользовательских монад. Выигрыш — один основной стиль effectful кода и отсутствие второго набора правил elaboration в первой реализации.
