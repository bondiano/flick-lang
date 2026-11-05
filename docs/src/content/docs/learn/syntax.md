---
title: Syntax in five minutes
description: Every construct of Flick's surface syntax, one short example each.
---

A quick tour for reading Flick code. The [grammar](../../reference/grammar/)
has the exact rules; this page shows each construct once.

## Values

```flick
let count = 42              // Int
let ratio = 3.14            // Float
let huge = 10n              // BigInt
let greeting = "Hello,\n\"world\""
let path = r"C:\temp"       // raw string: no escapes
let done = true             // Bool: true or false
let nothing = ()            // Unit
```

There is no string interpolation: join strings with `++`. Write `0.5`, not
`.5`: a `.` is always field access.

## Comments

```flick
// a line comment
/// a doc comment, used by `flick doc`
/* a block comment /* that nests */ */
let x = 1
```

## Names

- `user`, `load_user`: values, functions, fields and type variables start
  with a lowercase letter or `_`.
- `User`, `Some`, `App.Main`: types, constructors, modules, classes,
  services and errors start with a capital letter.
- `List.map` is a name from a module; `user.name` is a field. Neither has
  spaces around the `.`.

## Functions

A named function has its signature right above its `let`. Arguments are
separated by spaces, without parentheses or commas:

```flick
add : Int -> Int -> Int
let add x y = x + y

let three = add 1 2

increment : Int -> Int
let increment = add 1
```

Functions are curried: `add 1` is a function waiting for `y`. An anonymous
function is `fun`:

```flick
let doubled = List.map (fun x -> x * 2) [1, 2, 3]
```

## The main rule: parentheses only group

`f (a, b)` is **not** a call with two arguments. `(a, b)` is a tuple, so the
function gets one argument, and the space before the parenthesis changes
nothing:

```text
add 1 2        // 3
add (1, 2)     // FLICK-T003: expected `Int`, found `(Int, Int)`
add(1, 2)      // the same error
```

Parentheses are for grouping and for tuples: `show (add 1 2)`.

## Indentation instead of braces

The body of a function, a `match` arm or an `if` branch is the indented
lines below it. There is no `in` and no `return`: the last line of a block
is its value.

```flick
# type alias User = { name : String; age : Int }
#
describe : User -> String
let describe user =
  let years = show user.age
  user.name ++ ", " ++ years
```

A line indented deeper continues the line above; a line at the same column
starts the next statement. Tabs are an error.

## Pipelines

`x |> f` is `f x`. Long chains are read top to bottom:

```flick
total : List Int -> Int
let total xs =
  xs
  |> List.filter (fun x -> x > 0)
  |> List.sum
```

## Tuples, lists and records

```flick
let pair = (1, "one")
let numbers = [1, 2, 3]
let range = [1..10]

type alias User = { name : String; age : Int }

let ada : User = { name = "Ada"; age = 36 }
let older = { ada with age = ada.age + 1 }
```

Records are exact: a value with an extra field is not a `User`. Fields are
separated by `;` or a line break.

## Types

`type` declares variants (an ADT); `type alias` gives a type another name.
Type parameters follow the name.

```flick
type Shape = Circle Float | Rect Float Float | Empty
  deriving Eq, Show

type Tree a = Leaf | Node (Tree a) a (Tree a)
```

`Option a` (`None | Some a`) and `Result e a` (`Err e | Ok a`) are declared
the same way in the Prelude.

## Pattern matching

A `match` must cover every value; the compiler names a case you missed.

```flick
# type Shape = Circle Float | Rect Float Float | Empty
#
area : Shape -> Float
let area shape =
  match shape with
  | Circle r -> 3.14 * r * r
  | Rect w h -> w * h
  | Empty -> 0.0

kind_of : Shape -> String
let kind_of shape =
  match shape with
  | Rect w h when w == h -> "square"
  | Circle _ | Empty -> "round or nothing"
  | Rect _ _ -> "rectangle"

first_or : a -> List a -> a
let first_or fallback xs =
  match xs with
  | [] -> fallback
  | [x, ..._] -> x
```

`when` adds a guard, `|` joins alternatives, `_` matches anything and
`...rest` takes the rest of a list.

## Conditions

`if` is an expression, so both branches have the same type:

```flick
sign : Int -> String
let sign n = if n < 0 then "negative" else if n == 0 then "zero" else "positive"
```

## Effects and errors

The braces after an arrow list what a call needs at run time: the services
before `;`, the expected errors after it. A runtime function makes such calls
directly, as it calls a pure function.

```flick
# type alias User = { name : String; age : Int }
#
type error NotFound = NotFound String

service Users =
  get : String ->{; NotFound} User

greet : String ->{Users; NotFound} String
let greet id =
  let user = Users.get id
  "Hello, " ++ user.name

greet_or_guest : String ->{Users;} String
let greet_or_guest id =
  try greet id with
  | NotFound _ -> "Hello, guest"
```

`try … with` handles `NotFound`, so the error is gone from the row of
`greet_or_guest`. `task { … }` stores a computation without running it, and
`await` runs it:

```flick
# type error NotFound = NotFound String
# service Users =
#   get : String ->{; NotFound} String
# greet : String ->{Users; NotFound} String
# let greet id = Users.get id
#
twice : String ->{Users; NotFound} (String, String)
let twice id =
  let request = task { greet id }
  let first = await request
  (first, await request)
```

`a -> b` is pure, `a ->{} b` is a runtime call with nothing to provide: the
two are different types. [Effects, errors and services](../../reference/effects/)
explains the rows.

## Modules

A file starts with its module name and what it exposes, then its imports:

```flick
module App.Main exposing (main)

import App.Greeting exposing (greet)

main : Unit ->{; AppError} Unit
let main () =
  Platform.stdout_line (greet "Ada")
```

`import Effect.Log as Log` gives a module a shorter name. Built-in modules,
such as `List`, `String` and `Platform`, need no import.

## Next

- [Getting started](../../start/getting-started/): install Flick and run a
  program.
- [Learn Flick](../): the guided path through types, effects and services.
- [Keywords](../../reference/keywords/): every reserved word with an example.
- [Play](../../play/): try the syntax in the browser.
