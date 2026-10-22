# Regions and captures

A `scope` releases what it acquired when its body ends, and the checker
proves that nothing usable outlives it (§7 "Ресурсы", "Статические regions и
захваты"). Regions are checked after types and effects; there is no early
close and no linearity: a resource may be used any number of times while its
scope is open.

## Scopes and owners

```flick
read_config : Path ->{FileSystem; OpenError, ReadError} String
let read_config path =
  scope {
    let file = File.open_scoped path
    File.read_all file
  }
```

Each `scope { … }` makes a fresh region, a rigid type that no type outside
the scope may mention, and the owner of what is acquired inside. An
operation that acquires a resource of region `s` needs the service
`Scope s` of `Prelude`, and each `scope` provides `Scope` of its own region
only, the way `provide` provides a service. A call whose region is not known
otherwise acquires into the innermost `scope`. A function may pass the
requirement on to its caller's scope by naming it in its signature:

```flick
open_two : forall s. Path ->{FileSystem, Scope s; OpenError} (File s, File s)
let open_two path =
  (File.open_scoped path, File.open_scoped path)
```

`scope s { … }` names the region `s` in the types of its body and binds the
owner `s : Fiber.Owner s` in its values. A `scope` is a runtime construct: a
pure function cannot open one (`FLICK-E001`). A call that needs `Scope s`
outside every scope and without it in the signature is `FLICK-G004`.

A call needing `Scope s` of an outer region passes through the inner scopes
to the one that owns `s`, and the resource is released when that scope
closes. The compiler hands the call its owner lexically: the owner of a
region is never looked up by whatever scope happens to be innermost when
the call runs.

```flick
keep : forall s. Path ->{FileSystem, Scope s; OpenError} File s
let keep path =
  acquire : Unit ->{FileSystem, Scope s; OpenError} File s
  let acquire () = File.open_scoped path
  scope {
    let local = File.open_scoped path
    acquire ()
  }
```

Here `local` is released when the inner `scope` ends, and the file `keep`
returns when the caller's scope does.

## Captures

A resource has its region in its type: `File s`, and also `List (File s)`
or a nominal `Wrapper s` around one. A closure or a `task` holds the values
it uses, and with them their regions, which its own type may not show.
Such hidden captures are written as a qualifier:

| Type | Holds |
| - | - |
| `T^{s, t}` | at most the regions `s` and `t` beyond what `T` shows |
| `T^c` | at most the capture row `c` (kind `CaptureRow`) |
| `T^(c + d)` | at most both |
| `T` | nothing beyond what `T` shows |

```flick
read_later : forall s. File s -> (Eff none {ReadError} String)^{s}
let read_later file =
  task { File.read_all file }
```

A qualifier is an upper bound: a wider one is accepted, a narrower one than
what the value holds is `FLICK-G001`. Mentioning `s` in a parameter type
does not make a function hold `s`; using a value of it does. What a type
shows is held openly and needs no qualifier: `fun () -> file` has type
`Unit -> File s`.

Each stage of a curried function holds the parameters before it, as its
partial application does, and a signature says so without a qualifier:
`File s -> String -> String` is read as `File s -> (String -> String)^{s}`,
in `.iface` too. Only regions and capture rows are added this way; what a
value of a type variable holds is not known from the signature, so a stage
that hides one needs `a^c -> (b -> b)^c`.

Locals are inferred: a closure without an expected type gets the qualifier
of what it uses, and a local signature need not name the regions its
function holds. A top-level signature is checked as written, and `.iface`
keeps its qualifiers for importers.

| Construct | Holds |
| - | - |
| `fun`, `task` | the locals it uses |
| a stage of a curried function | the regions of the parameters before it, and the locals it uses |
| record, tuple, list, constructor | what its parts hold |
| `T^c` for a type variable `a^c` | at most `c`, whatever `a` stands for |
| type variable `a` without a qualifier | what `a` stands for: generic code that hides a value of `a` in a closure needs `a^c` |
| `Eff r e a`, a function | only its qualifier |

A function that takes a callback declares the callback's captures and keeps
them in what it returns, as `map_effect` does:

```flick
map_effect : forall c a b r e. (a ->{r; e} b)^c -> (List a ->{r; e} List b)^c
foreign sync map_effect from "./list.js" "mapEffect"
```

The parts of a value matched by a pattern have their own types: a
constructor's fields are checked against their types when it is built.

## What cannot leave a scope

| Path | Rule |
| - | - |
| the value of the scope, directly or inside data | its type may not mention the region (`FLICK-G002`) |
| a closure or `task` returned or stored | its qualifier may not hold the region (`FLICK-G002`, or `FLICK-G001` against a written type) |
| a nominal or opaque wrapper | the region stays among its type arguments |
| a cell or binding from outside the scope | its type may not come to mention the region; mutable storage keeps its captures exactly (`T003`) |
| an error | its payload holds no region, even when a handler inside the scope catches it (`FLICK-E005`); `type error E (s : Region) = E (File s)` is rejected where it is declared |
| a foreign function | its signature is its retention contract: what it keeps is in its result's qualifier |

## Children

```flick
spawn :
  forall s c r e a.
  Fiber.Outlives c s =>
  Fiber.Owner s ->
  (Failure e ->{r; none} Unit)^c ->
  (Eff r e a)^c ->{r; none} Fiber.Fiber e a
```

`Fiber.spawn owner supervisor task` registers `task` with `owner`; the
supervisor observes its failure. A child runs until its owner's scope
closes, so the task and the supervisor may hold only regions that end no
earlier: the owner's region, the regions of scopes around it, and what the
signature names when the owner is a local scope. `Fiber.Outlives c s` states
it; the compiler decides it and a program declares no instance of it. A
child that holds a shorter region is `FLICK-G003`.

## Not checked yet

| Construct | Today |
| - | - |
| capture-polymorphic signatures of the standard library | written by hand in each module until the library exists |
| a region variable unified late with another region in a capture row | the two are kept apart, which may reject a program |
