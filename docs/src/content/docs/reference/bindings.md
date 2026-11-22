---
title: "Bindings and local execution"
---

## Blocks

Every statement before the last must have type `Unit`; another value is
dropped only with `ignore` (`FLICK-T011`, with that fix). The last statement
is the value of the block and must be an expression (`FLICK-T019`).

## Pattern matching with `let ... else`

Inside an ordinary statement block, `let pattern = initializer else fallback`
matches `initializer` against `pattern` and, when it does not match, returns
`fallback` as the result of the remainder of the current block. It is exact
sugar for:

```text
match initializer with
| pattern -> <rest of current block>
| _ -> fallback
```

Statements before `let ... else` stay outside the `match`, `initializer` is
evaluated once, and `fallback` is a regular expression of the current block's
result type (it does not require `Never` and is not a nonlocal `return`).

```flick
type InputError = MissingName | MissingEmail
type alias RawInput = { name : Option String; email : Option String }
type alias User = { name : String; email : String }

build_user : RawInput -> Result InputError User
let build_user input =
  let Some name = input.name else
    Err MissingName

  let Some email = input.email else
    Err MissingEmail

  Ok { name = name; email = email }
```

When `let ... else` sits inside a nested block such as an outer `let`
initializer, mismatch only finishes that inner block and the surrounding block
continues with the fallback value:

```flick
# type alias User = { name : String }
# render : String -> String
# let render s = "Hello, " ++ s
#
greet : Option User -> String
let greet maybe_user =
  let name =
    let Some user = maybe_user else "guest"
    user.name
  render name
```

- **Scope and shadowing.** Pattern bindings are in scope only in the success
  remainder after `else`, never in `initializer` or `fallback`. If `pattern`
  shadows an outer local, `fallback` still sees the outer binding.
- **Monomorphic bindings.** Every name bound by `let ... else` is monomorphic,
  just like in `match` — even an irrefutable single-variable pattern
  `let x = v else fallback`. It never generalizes like ordinary `let x = v`.
  An irrefutable pattern emits `FLICK-T010` warning that `else` is unreachable,
  while retaining `match` typing.
- **GADT, row, class, mutation and resource rules.** GADT refinements,
  existentials, row equalities and held class dictionaries apply only in the
  success remainder. Earlier `let mut` assignments persist on both paths; no
  function, loop or handler boundary is added, so `break`, `continue`, `?`,
  `try` and `scope` behave identically to the equivalent `match`.
- **v1 boundaries.** `let ... else` requires a non-empty success remainder
  (`FLICK-T019` otherwise). It is not allowed at the module level, directly as
  a `do` or `for` clause (though ordinary `for ... do` loop bodies are
  allowed), with `rec`/`mut`/`lazy`/`assert`, with function parameters, with a
  paired or inline type signature, or with `when` guards (`FLICK-S007` /
  `FLICK-T006`).

## Variables

`let mut name = value` declares a variable of the function whose block holds
it, and `name = value`, a statement of type `Unit`, assigns it. A read sees
the last value assigned before it; a value read earlier stays as it was.

```flick
clamp : Int -> Int -> Int -> Int
let clamp low high n =
  let mut value = n
  if value < low then value = low
  if value > high then value = high
  value
```

The variable has one type, fixed where it is declared, by its signature or
its value; it is not generalized, and a value assigned to it is checked
against it (`FLICK-T003`). An assignment is not a runtime call: a pure
function may assign its variables. Only a name of one `let mut` of a block
is a variable: not a definition of the module, a function, a pattern,
`let assert`, or a binding of `do` or `for` (`FLICK-S016`). A parameter, a
plain `let` or a definition of the module is not assigned (`FLICK-T069`,
whose fix adds `mut` to a plain `let`); a `mut` never assigned is the warning
`FLICK-R020`, whose fix removes it.

A variable belongs to its function. A `fun`, a local function, a `task`, a
lazy `for`, the rest of a `do` block or an arm of a handler of operations
inside it may run later, again or at the same time, so none of them reads or
assigns the variable (`FLICK-G005`): the function stays pure from the outside.
Bind the value with `let` first and use that name. The tour has an
[example](../../tour/captures/) of this error. A `scope`, a `provide`, a
branch and an arm of `match` or of a `try` run in place and may assign. The
body of `try … with` and sequential loops may also assign. A failed
right-hand side leaves its target unchanged; a handler sees earlier completed
assignments. Strict `[for …]` comprehensions still reject external assignments
(`FLICK-T070`). Region safety follows the same typing-by-use rules as other
values (see [Regions](../regions/)).

## Local loops

`loop { … }` returns the value of `break value`; all exits agree on its type.
A bare `break` is `break ()`. `while condition do body` and `for … do body`
return `Unit`; the condition is `Bool` and their bodies return `Unit`.
`continue` skips the rest of an iteration. Unlabeled exits target the nearest
loop; a loop may carry a label (`'outer: loop { … }`, `'outer: while … do …`,
`'outer: for … do …`; the colon after the label is optional) so `break 'outer` or `continue 'outer` exits or
continues that enclosing loop instead, closing each scope it leaves before
continuing. Duplicate labels on enclosing loops and unknown labels are
`FLICK-T071`. A nested function, task, lazy sequence or operation handler
cannot exit an outer loop (`FLICK-T071`). `loop` and `while` are contextual
words: existing names such as `Loop.loop` and `Schedule.while` remain valid.
The tour has a runnable [example](../../tour/labels/).

```flick
module App.Count exposing (count)

count : Int -> Int
let count n =
  let mut i = 0
  while i < n do i = i + 1
  i
```

`break 'outer` leaves both loops at the first pair that adds up to the
target, so `first_sum 10 [1, 4, 7] [9, 3, 6]` is `1`:

```flick
first_sum : Int -> List Int -> List Int -> Int
let first_sum target xs ys =
  let mut found = -1
  'outer: for x <- xs do
    for y <- ys do
      if x + y == target then {
        found = x
        break 'outer
      }
  found
```

## Recursion

Top-level declarations may use each other in any order, but recursion is
explicit: a function that uses itself is `let rec`, and functions that use
each other are defined in one `rec { … }` group (`FLICK-T017`). Only functions
recurse; a value that would be evaluated while it is defined is `FLICK-T012`.

A recursive function may call itself at another type only when its
signature starts with a `forall` that binds all its variables; otherwise
that call is `FLICK-T015`. The members of a `rec` group call each other at
their own types without one, whatever their variables are named: `odd_len`
of `List b -> Bool` called from `even_len` of `List a -> Bool` on a
`List a` is the same type.

```flick
type Nested a = Flat a | Nest (Nested (List a))

depth : forall a. Nested a -> Float
let rec depth value =
  match value with
  | Flat _ -> 0
  | Nest inner -> 1 + depth inner
```

## Generalization

A binding is polymorphic only when it is built purely: no runtime call runs
while it is defined (see [Generalization in the presence of Ref](../../spec/03-types-and-classes/#generalization-in-the-presence-of-ref)). Lambdas and `task`
bodies are deferred and count as pure. A polymorphic signature, annotation or
higher-rank argument over a runtime result is `FLICK-T013`, and so is a
top-level value whose type stays unknown. Type variables of an enclosing
signature, or of a value already in scope, are never generalized, and mutable
containers are invariant. A row of calls still unknown when a binding is
generalized becomes `none`, as at the end of a declaration
([empty rows](../effects/#empty-rows-none)); it is not quantified.

```flick
make_empty : forall a. Unit ->{} Ref (Option a)
let make_empty () =
  Ref.make None

fresh_cell : forall a. Eff none none (Ref (Option a))
let fresh_cell =
  task { Ref.make None }
```

Classes and instances: [classes.md](../classes/).

## Not checked yet

The checker covers every construct of this page, the code of plugin blocks
included. The formal development covers less of it:

| Construct | Today |
| - | - |
| `let mut` | `formal/Formal/LetMut.lean` (T7) translates straight-line blocks with assignments, branches and `scope` into the typed core and derives purity and region safety from that; loops are outside the translation |
| loops, labels, `break`, `continue` | `formal/Formal/LocalControl.lean` proves laws of their completions: every exit carries the current variables, a failed right-hand side assigns nothing, a handler sees the variables as they are, an invariant each iteration keeps holds on every finite exit, a labeled exit leaves every loop up to its target, and the scopes it leaves run their finalizers newest first. These are laws of that algebra, not of the typed core: fixtures run the compiled loops on JS and Go, and no proof relates them to the compiler or a backend |
