---
title: "Regions and captures"
---

A `scope` releases what it acquired when its body ends, and the checker
proves that nothing usable outlives it (see [Resources](../../spec/07-errors-and-concurrency/#resources), [Static regions and captures](../../spec/07-errors-and-concurrency/#static-regions-and-captures)). Regions are checked after types and effects; there is no early
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
or a nominal `Wrapper s` around one. A closure, `task` or constructor can
also retain values whose regions its type does not show.

| Type | Bound |
| - | - |
| `T^{s, t}` | at most the regions `s` and `t` in the qualified part |
| `T^c` | at most the capture row `c` (kind `CaptureRow`) |
| `T^(c + d)` | at most the union of both rows |
| `T` | no extra hidden captures; captures exposed by `T` remain |

For a generic value, `a^c` bounds the **whole value**, not just captures
beyond `a`. At a value boundary, `(f a)^c` likewise bounds parameter-held
captures when `f` resolves to data that stores only its parameters.
Functions and types with hidden storage, such as `StateT`, instead retain
a hidden-capture qualifier. Explicit stored qualifications are part of
type identity, not bounds that may be discarded.

`Ref.make : forall c a. a^c ->{} Ref (a^c)` puts the stable capture capacity
in the invariant payload type. Widening a cell alias's outer value bound
cannot widen that inner bound to admit shorter-lived replacements.
The distinction survives [`.iface` schema 2](../iface/#types-and-kinds).

```flick
read_later : forall s. File s -> (Eff none {ReadError} String)^{s}
let read_later file =
  task { File.read_all file }
```

A bound can be wider than what a value holds; a narrower bound is
`FLICK-G001`. `a^none` means a generic value holds no captures, not that
substitution can erase a concrete `File s` or a closure's stored qualifier.
For a concrete type, regions already exposed in it remain even under
`^none`. Merely mentioning `s` in a parameter type does not make a function
hold `s`; using a value of it does. For example, `fun () -> file` has type
`Unit -> File s`.

Each stage of a curried function retains earlier parameters, including
their generic value captures. `File s -> String -> String` is read as
`File s -> (String -> String)^{s}`; `a -> b -> a` also retains the first
value's captures in its second stage. These projections survive `.iface`
serialization. Use `a^c -> (b -> b)^c` to name the bound explicitly.

Locals are inferred: a closure without an expected type gets the qualifier
of what it uses, and a local signature need not name the regions its
function holds. A top-level signature is checked as written, and `.iface`
keeps its qualifiers for importers.

| Construct | Holds |
| - | - |
| `fun`, `task` | the locals it uses |
| a stage of a curried function | earlier parameters' value captures and the locals it uses |
| record, tuple, list, constructor | what its parts hold |
| type variable `a^c` | the whole value holds at most `c`, without erasing concrete regions or stored qualifiers |
| type variable `a` without a qualifier | what the substituted type holds; curried stages retain its capture projection |
| `Eff r e a`, a function | hidden captures from its qualifier, not from a future result alone |

A function that takes a callback declares the callback's captures and keeps
them in what it returns, as `map_effect` does:

```flick
map_effect : forall c a b r e. (a ->{r; e} b)^c -> (List a ->{r; e} List b)^c
foreign sync map_effect from "./list.js" "mapEffect"
```

Prelude's `map`, `map2` and `flat_map` callbacks quantify an input capture
bound **per invocation**, independently of the container's bound `d`:
`(forall v. a^v -> b)^c` for `map`,
`(forall u v. a^u -> (b^v -> x)^(c + u))^c` for `map2`, and
`(forall v. a^v -> (m b)^(c + v))^c` for `flat_map`.
The second stage of `map2` retains its first input. This matters for
`StateT` and `ReaderT`: state or environment supplied when running may
hold regions not present when the computation was built.
`pure : forall c a. a^c -> (f a)^c` preserves its input bound too.
See the [user-defined transformer instances](../../contributing/implementation-decisions/#instances-of-user-types).

Constructors retain field captures. Pattern matching and record/tuple/list
projection propagate the enclosing bound to extracted values, including
closure fields; applying a delayed field also retains the supplied input's
captures. Explicit field qualifications are preserved, not overwritten by
the enclosing bound. Primitive fields such as `Number` gain no captures.

## What cannot leave a scope

| Path | Rule |
| - | - |
| the value of the scope, directly or inside data | its type may not mention the region (`FLICK-G002`) |
| a closure or `task` returned or stored | its qualifier may not hold the region (`FLICK-G002`, or `FLICK-G001` against a written type) |
| a nominal or opaque wrapper | a visible region or hidden field capture cannot be erased |
| a cell or binding from outside the scope | its type may not come to mention the region; mutable storage and its qualified element types are invariant (`FLICK-T003`), even when a value bound could otherwise widen |
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
| foreign retention contracts | signatures are written and reviewed by hand; the checker does not inspect foreign bodies |
| a region variable unified late with another region in a capture row | the two are kept apart, which may reject a program |
