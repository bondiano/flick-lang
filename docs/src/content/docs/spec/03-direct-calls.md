---
title: "3. Direct effectful calls"
---

An effectful function is called in an ordinary expression, and `let` binds its result. A sequence of operations is written with ordinary expressions and bindings.

```flick
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

Function blocks and `match` are delimited by indentation. `in` is not needed; the last expression is the result of the block. A standalone intermediate expression must have type `Unit`; any other result is discarded explicitly with `ignore`. Curly braces are used for records and the special blocks `task`, `scope`, `sql`, `ui`. Inside them, line breaks separate bindings and expressions; `;` is allowed for compact notation. Arguments and fields are evaluated left to right. Self-recursion is marked with `let rec`; mutual recursion of functions uses `rec { ... }`, where each definition has its own full signature. Recursive values with immediate evaluation are forbidden in the first version.

## A full signature above every function

Every named function has a full signature directly above its definition. The rule is the same for public, internal and local functions, instance methods and functions obtained by partial application. The `let` header keeps the argument names; a second style with parameter and result types in the header is not used.

```flick
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

New free type variables in a function signature are implicitly universally quantified; the kind is inferred from the position. In a nested signature, the variable names of the outer signature denote the same variables. The signature is checked and does not cancel the value restriction, the invariance of mutable containers or lifetime restrictions. Polymorphic recursion is allowed only with an explicitly written full `forall` on the recursive function: the checker checks the body against the signature but does not infer it. Instantiation remains predicative. The type of an instance method is checked against the class signature after substituting its parameters; new requirements on the calling code cannot be added silently.
