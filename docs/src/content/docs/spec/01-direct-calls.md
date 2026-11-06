---
title: "Direct effectful calls"
source_hash: 5fffa9e4eb5df76c
---

An effectful function is called in an ordinary expression, and `let` binds its result. A sequence of operations is written with ordinary expressions and bindings.

```flick
# type UserId = UserId String
# type alias User = { id : UserId; name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# service Log =
#   info : String ->{} Unit
# normalize_name : String -> String
# let normalize_name name = String.trim name
#
load_name : UserId ->{Users, Log; NotFound} String
let load_name id =
  let user = Users.get id
  let name = normalize_name user.name

  Log.info name

  name
```

The arrow annotation is `a ->{r; e} b`, where `r` is the service requirements and `e` is the expected errors. The kind of arrow denotes a runtime call; a plain `a -> b` denotes a pure call. Empty `r` and `e` do not make a runtime call pure.

The annotation is written together with the arrow: `->{r; e}`, `->{}`. An arrow with a space before the curly brace, `a -> { name : String }`, is an ordinary pure arrow whose result is a record type. This way a runtime arrow and a record are told apart by tokens alone, without context.

```text
normalize_name : String -> String

Users.get :
  UserId ->{Users; NotFound} User

load_name :
  UserId ->{Users, Log; NotFound} String
```

The result of the expression `Users.get id` is a `User`, and the effect of computing it is accounted for separately. The compiler generates the necessary binding, including for standalone expressions such as `Log.info name`.

`let` binds the value of the expression without any additional transformation: a value of type `Eff` stays `Eff`, `Option` stays `Option`, `List` stays `List`, `Result` stays `Result`. The effect of the right-hand side is determined by the original signature of the called function.

Function blocks and `match` are delimited by indentation. `in` is not needed; the last expression is the result of the block. A standalone intermediate expression must have type `Unit`; any other result is discarded explicitly with `ignore`. Curly braces delimit records and the blocks of `task`, `scope`, `provide`, `sql`, `ui`, `cli`, `codec`, `do`, `for` and `rec` (see [Layout](../../reference/grammar/#layout)). Inside them, line breaks separate bindings and expressions; `;` is allowed for compact notation. Arguments and fields are evaluated left to right. Self-recursion is marked with `let rec`; mutual recursion of functions uses `rec { ... }`, where each definition has its own full signature. A recursive value that would be evaluated while it is defined is an error (`FLICK-T012`).

## A full signature above every function

Every named function has a full signature directly above its definition. The rule is the same for public, internal and local functions, instance methods and functions obtained by partial application. The `let` header keeps the argument names; a second style with parameter and result types in the header is not used.

```flick
# type UserId = UserId String
# type alias User = { id : UserId; name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# service Log =
#   info : String ->{} Unit
# type alias UserView = { id : UserId; title : String }
#
render : Show a => a -> String
let render value =
  show value

to_view : User -> UserView
let to_view user =
  {
    id = user.id;
    title = String.trim user.name;
  }

load_view : UserId ->{Users, Log; NotFound} UserView
let load_view id =
  let user = Users.get id
  let view = to_view user
  Log.info view.title
  view
```

The signature contains the constraints, the whole function type and the effect of every arrow. For example, `A -> B ->{R; E} C` means a pure partial application of the first argument and a runtime call after the second. For `A ->{R; E} (B -> C)`, the runtime call happens already after the first argument.

```flick
greet : String -> String -> String
let greet prefix name =
  prefix ++ ", " ++ name

hello : String -> String
let hello =
  greet "Hello"
```

For an anonymous lambda, the kind of each arrow and both rows are inferred, or it is checked against the expected type. `fun x -> normalize_name x` is a pure function, `fun x -> task { Users.get x }` is a pure `Eff` builder, and `fun x -> Users.get x` is a runtime function. With nested lambdas, the effect belongs to the stage that performs it. Bindings that are not functions do not have to have a separate signature; local type hints remain available.

A pure body or callback is allowed in an expected runtime position through a checked lift node in the IR: the backend defers the body and wraps its result in Effect. This is an adaptation of the representation, not an equality of arrows. Lift does not run a returned `Eff` value and does not remove an additional layer. The reverse conversion of a runtime call into a pure one by empty rows is forbidden; captures are preserved by lift.

## Sync stages

A runtime arrow whose effects are a closed row of effects with `tail` and `abort` operations, services and errors, is a *Sync* stage: it runs at once, without suspending. Suspension shows in the row: the built-in effect `Async`, of class `control`, which the runtime's scheduler handles at the root. A function that waits for time, input or output or another fiber has `Async` (`Effect.sleep : Duration ->{Async;} Unit`), and `->{}` is Sync. A `foreign async` always needs `Async`; a `foreign effect` is as written: without `Async` it runs at once, and its suspending is a defect. For now the JavaScript backend compiles to a plain JS function without `Effect.gen` only a row of `Atom.Read` (`Atom a ->{Atom.Read;} a`, the render of `Ui.view`, the computation of `Atom.derived`) with no errors; the rest of Sync stays an Effect; its calls of other Sync functions and of the operations of `Atom.Read` are plain calls, which read the services of the context the runtime set around the call (the reader of a render or a derived atom). The stage is decided where the arrow is written, so a row variable solved to `Atom.Read` later, as in `forall r. (a ->{r} b) -> …`, keeps the runtime stage its function was compiled with. A foreign function returns an Effect, so its arrows are never Sync.

```flick
# count : Atom.Writable Float
# let count = Atom.make 0
#
doubled : Unit ->{Atom.Read;} Float
let doubled () = Atom.value count * 2

twice : (Unit ->{r; e} Float) -> Unit ->{r; e} Float
let twice f () = f () + f ()

quadrupled : Unit ->{Atom.Read;} Float
let quadrupled () = twice doubled ()
```

The lift of the previous section covers every pair of stages: a pure function is called as a Sync one is; a Sync function in a runtime position becomes an Effect that calls it with the fiber's services (`doubled` above); a runtime function in a Sync position, or a runtime call in a Sync stage (`twice doubled ()`), runs its effect at once with the services of the context. A runtime call there that suspends is a defect, as is a render that awaits. Sync functions of the same type unify only with Sync ones.

New free type variables in a function signature are implicitly universally quantified; the kind is inferred from the position. In a nested signature, the variable names of the outer signature denote the same variables. The signature is checked and does not cancel the value restriction, the invariance of mutable containers or lifetime restrictions. Polymorphic recursion is allowed only with an explicitly written full `forall` on the recursive function: the checker checks the body against the signature but does not infer it. Instantiation remains predicative. The type of an instance method is checked against the class signature after substituting its parameters; new requirements on the calling code cannot be added silently.
