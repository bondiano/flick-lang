---
title: "6. Higher-order functions and collection traversal"
---

## Pure and effectful traversal

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

In the first version, `List.map` takes a pure callback, `List.map_effect` runs a runtime callback sequentially from left to right, and `parallel_all` requires an explicit concurrency limit. Building `requests` does not start the requests. There is no single effect-polymorphic `map`.

```text
List.map :
  forall c a b.
  (a -> b)^c -> (List a -> List b)^c

List.map_effect :
  forall c a b r e.
  (a ->{r; e} b)^c ->
  (List a ->{r; e} List b)^c
```

Partial application retains the callback and its `c`. After an eager traversal, the resulting `List b` keeps the captures of `b` itself, but need not retain the already executed callback. This is more precise than the general `Functor.map`, which allows deferred application.

The alternative is an effect-polymorphic `map` whose effect depends on the callback. Then a calling convention has to be chosen for the cases where the callback returns an ordinary JS value or an Effect: specialization, a uniform representation or an additional abstraction. An example of such typing is in [Koka](https://koka-lang.github.io/koka/doc/book.html).

## Working with Monad without a special do

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

The same is expressed with ordinary `Option.bind` and `Option.map`. This is pure work with data; it does not need the Effect runtime. HKT and type classes are available without a generic `do`.

Generic `do` and `let*` are not part of the first version. Direct runtime calls use an ordinary `let`, cold descriptions use `task`, and running a stored description in the current fiber uses `await`. For user-defined monads, ordinary combinators remain.

The cost of the decision is more explicit code for parsers and user-defined monads. The gain is one main style of effectful code and no second set of elaboration rules in the first implementation.
