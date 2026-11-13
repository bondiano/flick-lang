---
title: "Bindings and local execution"
---

## Blocks

Every statement before the last must have type `Unit`; another value is
dropped only with `ignore` (`FLICK-T011`, with that fix). The last statement
is the value of the block and must be an expression (`FLICK-T019`).

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
Bind the value with `let` first and use that name. A `scope`, a `provide`, a
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
`continue` skips the rest of an iteration. Exits target the nearest loop,
without labels, and close each scope they leave before continuing. A nested
function, task, lazy sequence or operation handler cannot exit an outer loop
(`FLICK-T071`). `loop` and `while` are contextual words: existing names such
as `Loop.loop` and `Schedule.while` remain valid.

```flick
module App.Count exposing (count)

count : Int -> Int
let count n =
  let mut i = 0
  while i < n do i = i + 1
  i
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
([effects](../effects/)); it is not quantified.

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

| Construct | Today |
| - | - |
| `sql`, `ui`, `codec`, `schema` blocks | any type |
