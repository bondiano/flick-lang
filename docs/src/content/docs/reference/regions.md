---
title: "Regions and captures"
---

A `scope` releases what it acquired when its body ends, and the checker
proves that nothing uses it afterwards (see [Resources](../../spec/05-errors-and-concurrency/#resources), [Static regions and captures](../../spec/05-errors-and-concurrency/#static-regions-and-captures)). Regions are checked after types and effects; there is no early
close and no linearity: a resource may be used any number of times while its
scope is open. What is checked is where a value **uses** a region, not what
it **holds**: a value may keep a handle past the end of its scope, provided
nothing uses it there.

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

## Holding is not using

A closure, a `task` or a constructor may hold a handle past the end of its
`scope` when nothing it runs uses the handle there:

```flick
# type File (s : Region)
#
# service Files =
#   open_scoped : forall s. String ->{Scope s} File s
#
held_task : String ->{Files} Eff none none Unit
let held_task path = scope {
  let file = Files.open_scoped path
  task { ignore file }
}
```

A use that outlives its region is `FLICK-G002`:

```flick fail G002
# type File (s : Region)
#
# type error ReadError = ReadError String
#
# service Files =
#   open_scoped : forall s. String ->{Scope s} File s
#   read_all : forall s. File s ->{; ReadError} String
#
read_later : String ->{Files} (Unit ->{Files; ReadError} String)
let read_later path = scope {
  let file = Files.open_scoped path
  fun () -> Files.read_all file
}
```

```text
FLICK-G002: this uses region `s`, which the expected type `Unit ->{Files; ReadError} String` does not keep live: a function or a task uses a region only while the region is live
```

What a stage uses of a region shows in its row as `Live`, an element no
program writes: the checker reads it into signatures (elision) and leaves it
out of messages, interfaces and emitted code.

- **A runtime stage of a signature uses what its parameters reach.** Each
  runtime arrow and `Eff` needs live what the parameters before it reach:
  `read_all : File s ->{; ReadError} String` needs `s` live where it is
  called, and so does the stage inside a function that returns a closure
  over its parameter.
- **A pure operation uses no region.** A pure stage cannot reach a
  resource, so a lazy `for` sequence over a scoped file may leave its scope;
  the strict `[for …]` builds its list inside.
- **A `scope` is where its region is live.** `Scope s` in a row keeps `s`
  live, and the end of `scope s` discharges `Live s`. A row that uses a
  region and does not keep it live is `FLICK-G002`. `Live s` gives no
  `Scope s`: a function that acquires into a region names `Scope s`
  (`FLICK-G004`).
- **A service a region indexes keeps it live.** `Setup s` of a component's
  setup keeps the instance's region live, as `Scope s` does, so a setup may
  run `Ui.on_mount`, `Ui.watch` and `Atom.value` of its instance. The runtime
  provides `Setup s` only while the instance's region is open, and a handler
  a program installs for such a service needs its region live where it is
  installed (`FLICK-G002`).

## Qualifiers

| Type | What using it needs |
| - | - |
| `T^{s, t}` | the regions `s` and `t` live |
| `T^c` | the capture row `c` (kind `CaptureRow`) live |
| `T^(c + d)` | both rows live |
| `T` | nothing beyond what `T` shows |

A qualifier says what using a value needs, not what it holds: calling a
function `(τ₁ → τ₂)^C`, awaiting a task `(Eff r e τ)^C` or running a
`(Stream r e a)^C` needs `C` live there. A narrower qualifier hides such a
need and is `FLICK-G001`; a wider one is allowed. The parts of data take its
qualifier: a function taken out of `(Option (Unit -> Unit))^C` needs `C`
live when it is called.

```flick
# import File exposing (File, FileSystem, ReadError)
#
read_later : forall s. File s -> (Eff {Async, FileSystem} {ReadError} String)^{s}
let read_later file =
  task { File.read_all file }
```

A type variable is no use of what it stands for: generic code cannot take a
value of `a` apart, and a type argument carries its own qualifier, so
`List a` holds an `a` without saying so. `Ref.make : forall c a. a^c ->{}
Ref (a^c)` keeps the capacity of a cell in its invariant payload type:
widening an alias of the cell cannot let a shorter-lived replacement in.
The distinction survives [`.iface` schema 2](../iface/#types-and-kinds).

A function that takes a callback says what using its result needs, as
`map_effect` does:

```flick
map_effect : forall c a b r e. (a ->{r; e} b)^c -> (List a ->{r; e} List b)^c
foreign map_effect from "./list.js" "mapEffect"
  retention call
```

Prelude's `map`, `map2` and `flat_map` callbacks quantify an input bound
**per invocation**, independently of the container's bound `d`:
`(forall v. a^v -> b)^c` for `map`,
`(forall u v. a^u -> (b^v -> x)^(c + u))^c` for `map2`, and
`(forall v. a^v -> (m b)^(c + v))^c` for `flat_map`. What such a callback
returns holds nothing of its input, so `Option.map` gives `(Option b)^c`;
a lazy `Seq.map`, an `Eff` and `Result.map`, which passes its error on, keep
their source and give `^(c + d)`. `pure : forall c a. a^c -> (f a)^c` preserves its
input bound. See the [user-defined transformer instances](../../contributing/implementation-decisions/#instances-of-user-types).

## What cannot leave a scope

| Path | Rule |
| - | - |
| the value of the scope, directly or inside data | its type may not mention the region (`FLICK-G002`) |
| a closure or `task` returned or stored | no row of it may need the region live (`FLICK-G002`) |
| a value an acquisition gives | `Resource.acquire_release` gives `a^{s}`, a value of region `s` (`FLICK-G002`); a function it is passed to takes `Conn^c` for some `c` |
| a cell or binding from outside the scope | its type may not come to mention the region; mutable storage and its qualified element types are invariant (`FLICK-T003`), even when a value bound could otherwise widen |
| a `let mut` variable from outside the scope | its type is fixed where it is declared; a stored callable retains its use qualifier, so assigning it cannot permit a use after its region ends |
| an error | its payload holds no region, even when a handler inside the scope catches it (`FLICK-E005`); `type error E (s : Region) = E (File s)` is rejected where it is declared |
| a foreign function | its signature is its retention contract: what using its result needs is in the result's qualifier; with `retention owner` and one region in the signature, it gets `Fiber.Outlives` for a capture row it keeps and `Fiber.Provided` and `Detached` for the services of kept work; with two regions the signature writes them (`FLICK-F002`) |

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

The value has the lease's region `t`, so the callback can neither give it
back nor keep using it (`FLICK-T007`, `FLICK-G002`). An acquisition acquires
into a region `t` of its own, which ends when the resource is released: a
value of a type that names its region, such as `File t`, cannot be kept in
them. `RcRef`, `RcMap` and `Pool` lend their value to the `scope` of the
call (`Scope t`) and keep it until that scope closes, so their `get` gives
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
closes, so each region the task and the supervisor use must end no
earlier: the owner's region, the regions of scopes around it, and what the
signature names when the owner is a local scope. `Fiber.Outlives c s` states
it; the compiler decides it and a program declares no instance of it. A
child that uses a shorter region is `FLICK-G003`. The child runs with the
services of the place that spawned it, so they must outlive the owner as
well: `Fiber.Provided r s` (see [Effects](../effects/), "Layers
and `provide`").

A child acquires into its owner's region: a task that acquires names
`Scope s` of its owner, and one that names another region is
`FLICK-T008`. Work kept until an owner closes requires `Fiber.Outlives` the
same way: a finalizer (`Scope.add_finalizer`), the release of
`Resource.acquire_release`, an event handler (`Events.listen`), the source
of a background `Stream` or `Channel`, and the work a component starts with
`Ui.on_mount` and `Ui.watch`, which runs until its instance unmounts.

## Compared with Rust

| Rust | Flick |
| - | - |
| a lifetime `'a` | a region: the one a `scope` opens, or a parameter `forall s` |
| `T: 'a` | `Fiber.Outlives c s` for the capture row `c` of the work an owner keeps; the checker decides it, and a program declares no instance |
| `fn(&'lt ())` | `File inner -> Unit` |
| `for<'r> fn(&'r ())` | `forall r. File r -> Unit` |

Rust decides `T: 'a` syntactically, as RFC 1214 made it: every lifetime that
the type mentions or hides must outlive `'a`, including the argument of a
function pointer that the value never calls. The comment in rustc's own
`outlives` tests calls that rule stricter than it needs to be: the function
pointer is itself `'static`, so it could be bivariant in its argument. Flick
reads it the way that comment prefers: holding `File inner -> Unit` uses
nothing, so such a function may leave the scope of `inner`. What makes it
sound is not variance but a proof of the typing judgment: `T4'` in
`formal/Formal/Live/T4Live.lean` shows that a program typed this way reaches
no step that needs a region after its `scope` has closed, and no operation
answered by a handler of a closed region. The compiler is compared with the
model by tests, not proved equal to it.

Rust accepts `outlives::<'static, _>((|_| ()) as fn(&'lt ()))` because an
argument of type `&'static ()` fits a parameter of `&'lt ()`
(contravariance), so `T` is inferred as `fn(&'static ())`. Flick has no
subtyping between regions: `File inner` is no `File outer` (`FLICK-T008`).
What it has instead is a region parameter, `forall r. File r -> Unit`.

## Not checked yet

| Construct | Today |
| - | - |
| foreign retention contracts | signatures are written and reviewed by hand; the checker requires `retention`, and derives or requires `Fiber.Outlives` for what `retention owner` keeps, but does not inspect foreign bodies |
| a region variable unified late with another region in a capture row | the two are kept apart, which may reject a program |
