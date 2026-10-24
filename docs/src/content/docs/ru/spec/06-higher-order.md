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

То же выражается обычными `Option.bind` и `Option.map`. Это чистая работа с данными; среда выполнения Effect ей не требуется. HKT и классы типов доступны без обобщённого `do`.

Обобщённые `do` и `let*` в первую версию не входят. Прямые эффектные вызовы используют обычный `let`, холодные описания строятся через `task`, а сохранённое описание выполняется в текущем файбере через `await`. Для пользовательских монад остаются обычные комбинаторы.

Из-за этого код парсеров и пользовательских монад становится более явным. Зато у эффектного кода один основной стиль, а первой реализации не нужен второй набор правил преобразования синтаксиса в типизированное представление.
