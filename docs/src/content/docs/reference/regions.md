---
title: "Regions and captures"
---

A `scope` releases what it acquired when its body ends, and the checker
proves that nothing usable outlives it (see [Resources](../../spec/05-errors-and-concurrency/#resources), [Static regions and captures](../../spec/05-errors-and-concurrency/#static-regions-and-captures)). Regions are checked after types and effects; there is no early
close and no linearity: a resource may be used any number of times while its
scope is open.

## Scopes and owners

```flick
# import File exposing (Path, FileSystem, OpenError, ReadError)
#
read_config : Path ->{Async, FileSystem; OpenError, ReadError} String
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
# import File exposing (File, Path, FileSystem, OpenError)
#
open_two : forall s. Path ->{Async, FileSystem, Scope s; OpenError} (File s, File s)
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
# import File exposing (File, Path, FileSystem, OpenError)
#
keep : forall s. Path ->{Async, FileSystem, Scope s; OpenError} File s
let keep path =
  acquire : Unit ->{Async, FileSystem, Scope s; OpenError} File s
  let acquire () = File.open_scoped path
  scope {
    let local = File.open_scoped path
    acquire ()
  }
```

Here `local` is released when the inner `scope` ends, and the file `keep`
returns when the caller's scope does.

`scope s { body }` stands for `Scope.run (fun s -> body)`. `Scope.run` takes a
function polymorphic in the region, so a function can receive the body as a
value:

```flick
with_region : (forall s. Fiber.Owner s ->{Scope s; none} String) ->{} String
let with_region body = Scope.run body
```

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
# import File exposing (File, FileSystem, ReadError)
#
read_later : forall s. File s -> (Eff {Async, FileSystem} {ReadError} String)^{s}
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
serialization. A qualifier that says just this is noise (`FLICK-T059`); write
one to say less: `a -> (b -> b)^{}` holds nothing of the first value.

Locals are inferred: a closure without an expected type gets the qualifier
of what it uses, and a local signature need not name the regions its
function holds. The qualifiers of its later stages are bounds as written:
`wrap : forall a. a -> (Unit ->{} Unit)` lets the closure it returns hold
`a` only, so name the scope (`scope s { … }`) and write
`(Unit ->{} Unit)^{a, s}` when it holds a resource of `s`. A top-level
signature is checked as written, and `.iface` keeps its qualifiers for
importers.

| Construct | Holds |
| - | - |
| `fun`, `task` | the locals it uses |
| a stage of a curried function | earlier parameters' value captures and the locals it uses |
| record, tuple, list, constructor | what its parts hold |
| type variable `a^c` | the whole value holds at most `c`, without erasing concrete regions or stored qualifiers |
| type variable `a` without a qualifier | what the substituted type holds; curried stages retain its capture projection |
| `Errors e`, `Exit e a`, `Failure e` | what the errors of `e` hold; an error-row variable `e` is held as a type variable is: curried stages retain it, and `(Errors e)^c` bounds it elsewhere |
| `Eff r e a`, a function | hidden captures from its qualifier, and what its result type (and a function's parameter type) shows: a dropped result does not take a capture with it |

A function that takes a callback declares the callback's captures and keeps
them in what it returns, as `map_effect` does:

```flick
map_effect : forall c a b r e. (a ->{r; e} b)^c -> (List a ->{r; e} List b)^c
foreign map_effect from "./list.js" "mapEffect"
  retention call
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
the enclosing bound. Primitive fields such as `Float` gain no captures.

## What cannot leave a scope

| Path | Rule |
| - | - |
| the value of the scope, directly or inside data | its type may not mention the region (`FLICK-G002`) |
| a closure or `task` returned or stored | its qualifier may not hold the region (`FLICK-G002`, or `FLICK-G001` against a written type) |
| a value an acquisition gives | `Resource.acquire_release` gives `a^{s}`, a value of region `s` (`FLICK-G002`); a function it is passed to takes `Conn^c` for some `c` |
| a nominal or opaque wrapper | a visible region or hidden field capture cannot be erased |
| a cell or binding from outside the scope | its type may not come to mention the region; mutable storage and its qualified element types are invariant (`FLICK-T003`), even when a value bound could otherwise widen |
| an error | its payload holds no region, even when a handler inside the scope catches it (`FLICK-E005`); `type error E (s : Region) = E (File s)` is rejected where it is declared |
| a foreign function | its signature is its retention contract: what it keeps is in its result's qualifier; with `retention owner` and one region in the signature, it gets `Fiber.Outlives` for a capture row it keeps and `Fiber.Provided` and `Detached` for the services of kept work; with two regions the signature writes them (`FLICK-F002`) |

## Leases

`ScopedRef`, `Resource` and `ScopedCache` own resources of their own: a
replacement (`set`, `refresh`) or an eviction releases one while their
region is still open. Their value is used only under a lease: `use` runs a
callback with it, and the previous resource is released once no `use`
holds it.

```flick
# import ScopedRef exposing (ScopedRef)
#
use : forall c s r e a b. ScopedRef s a -> (forall t. a^{t} ->{r; e} b)^c ->{r; e} b
# let use = ScopedRef.use

from_acquire :
  forall c s r e a. (forall t. Eff {Scope t | r} e a^{t})^c ->{Scope s | r; e} ScopedRef s a
# let from_acquire = ScopedRef.from_acquire
```

The value holds the lease's region `t`, so the callback can neither give it
back nor keep it (`FLICK-T007`, `FLICK-G002`). An acquisition acquires into
a region `t` of its own, which ends when the resource is released: a value
of a type that names its region, such as `File t`, cannot be kept in them.
`RcRef`, `RcMap` and `Pool` lend their value to the `scope` of the call
(`Scope t`) and keep it until that scope closes, so their `get` gives
`a^{t}`.

## Children

```flick
spawn :
  forall s c r e a.
  (Fiber.Outlives c s, Fiber.Provided r s, Detached r) =>
  Fiber.Owner s ->
  (Failure e ->{Scope s | r; none} Unit)^c ->
  (Eff {Scope s | r} e a)^c ->{r; none} Fiber.Fiber e a
# let spawn owner supervisor child = Fiber.spawn owner supervisor child
```

`Fiber.spawn owner supervisor task` registers `task` with `owner`; the
supervisor observes its failure. A child runs until its owner's scope
closes, so the task and the supervisor may hold only regions that end no
earlier: the owner's region, the regions of scopes around it, and what the
signature names when the owner is a local scope. `Fiber.Outlives c s` states
it; the compiler decides it and a program declares no instance of it. A
child that holds a shorter region is `FLICK-G003`. The child runs with the
services of the place that spawned it, so they must outlive the owner as
well: `Fiber.Provided r s` (see [Effects](../effects/), "Layers
and `provide`").

A child acquires into its owner's region: a task that acquires names
`Scope s` of its owner, and one that names another region is
`FLICK-T008`. Work kept until an owner closes requires `Fiber.Outlives` the
same way: a finalizer (`Scope.add_finalizer`), the release of
`Resource.acquire_release`, an event handler (`Events.listen`), the source
of a background `Stream` or `Channel`.

## Not checked yet

| Construct | Today |
| - | - |
| foreign retention contracts | signatures are written and reviewed by hand; the checker requires `retention`, and derives or requires `Fiber.Outlives` for what `retention owner` keeps, but does not inspect foreign bodies |
| a region variable unified late with another region in a capture row | the two are kept apart, which may reject a program |
