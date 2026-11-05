---
title: "Higher-order functions and collection traversal"
---

## Pure and effectful traversal

```flick
# type UserId = UserId String
# type alias User = { id : UserId; name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# normalize_name : String -> String
# let normalize_name name = String.trim name
#
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

`List.map` takes a pure callback, `List.map_effect` runs a runtime callback sequentially from left to right, and `parallel_all` requires an explicit concurrency limit. Building `requests_for user_ids` does not start the requests. There is no single effect-polymorphic `map`.

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

Keeping the two families apart gives every callback one calling convention: a pure callback returns a plain value, a runtime callback an Effect.

## `do`, `for` and comprehensions

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

With `do` the nested matches become bindings:

```flick
pair_optional : Option a -> Option b -> Option (a, b)
let pair_optional left right =
  do {
    x <- left
    y <- right
    pure (x, y)
  }
```

`do` is sugar over the `Monad` of the Prelude: `p <- e` binds through `flat_map`, a bare expression is sequenced and discarded, `if c` is a guard through `Alternative`, `let` scopes over the rest of the block, and the last expression is the result. Continuations are pure functions, as `flat_map` requires: a runtime call inside a `do` over `Option` is an error (`FLICK-E001`) that suggests binding it before the block. A refutable pattern after `<-` is an error; use `match`. `do` over `Eff` is allowed and gets the lint `FLICK-T029` with a fix to a `task` with direct calls, which join rows that `Monad (Eff r e)` keeps fixed.

Bindings joined with `and`, `x <- a and y <- b`, run independently: neither value sees what the other binds, and `map2` of `Applicative` combines them. A group followed only by `pure e` needs `Applicative` alone, so it fits types that are not monads, such as a validation that collects every error; with more statements after it, the rest is `flat_map` over the tuple of the group. Over `Eff`, `map2` runs the two one after the other; running them concurrently stays explicit with `parallel`.

`for { x <- xs; if p x; y <- ys x } yield e` is a lazy `Seq` compiled to one native generator; `[for … yield e]` is the strict `List`, compiled to a loop. `x <- source` takes the elements of an `Iterable` source (`List`, `Seq`), and a refutable pattern there skips the elements that do not match; `if c` skips the element unless `c`, and `let` scopes over the rest. Without braces the clauses are separated by `;`, and a guard's `if` starts a new one: `[for x <- xs if x > 0 yield x]`. The body of a generator makes Sync calls at most (`->{Atom.Read;}`, [Sync stages](../01-direct-calls/#sync-stages)), and only where the stage that builds it may: each step of a lazy `Seq` reads the services of that stage, captured when the `Seq` is built, and a strict list reads them at once. A runtime call may suspend, so it is an error there; iterating a `Seq` again runs its Sync calls again, reading the atoms as they are then. A `Seq` keeps the captures of what it uses, since it runs later; one whose steps read atoms holds the reader of the function that builds it, which ends when that function returns, so the sequence cannot leave it (`FLICK-G001`, `FLICK-G002`): iterate it there, pass it to a function that consumes it, or build a list.

`for … do body` in place of `yield e` runs the body, a `Unit` block, for each element right there. Its clauses and body may make runtime calls, which join the rows of the enclosing function.

The cost is a second style next to direct calls; lints keep effectful code on direct calls. The gain is short code for `Option`, `Result`, lists, parsers and user-defined monads.
