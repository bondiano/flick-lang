---
title: "Live regions"
description: "An experimental discipline under which a region must be live where a value uses it, not where the value holds it."
---

By default the checker asks what a closure, a task or a constructor **holds**: a value that keeps a handle of a `scope` cannot leave that scope, though it never uses it (see [Regions and captures](../../reference/regions/)). Live regions ask instead where a value **uses** a region. A function that only carries a file around may then cross the end of the scope that owns it; one that reads the file may not.

The mode is experimental: its rules may change, and no release turns it on by itself.

## Turning it on

```toml
[build]
regions = "live"
```

| `build.regions` | What it checks |
| - | - |
| `"captures"` | what a value holds, as [Regions and captures](../../reference/regions/) describes; the default |
| `"live"` | what a stage uses of a region |
| anything else | `FLICK-P005` |

The mode covers the modules of the build. The built-in modules and `std` are checked under `"captures"` either way.

## Holding is not using

```flick live
# module App.LiveHeld exposing (..)
#
# type File (s : Region)
#
# service Files =
#   open_scoped : forall s. String ->{Scope s;} File s
#
held_task : String ->{Files;} Eff none none Unit
let held_task path = scope {
  let file = Files.open_scoped path
  task { ignore file }
}
```

The task leaves the `scope` holding `file`, and its row is empty: nothing it runs uses `s`. Under the default discipline this program is `FLICK-G001`, since the task holds what its empty row does not allow.

What uses a region stays inside its scope, and reads the same in both disciplines:

```flick
# module App.LiveUse exposing (..)
#
# type File (s : Region)
#
# type error ReadError = ReadError String
#
# service Files =
#   open_scoped : forall s. String ->{Scope s;} File s
#   read_all : forall s. File s ->{; ReadError} String
#
used_inside : String ->{Files; ReadError} String
let used_inside path = scope {
  let file = Files.open_scoped path
  let read = fun () -> Files.read_all file
  let later = task { Files.read_all file }
  ignore later
  read ()
}
```

A use that outlives its region is `FLICK-G002`:

```flick live fail
# module App.LiveAfterClose exposing (..)
#
# type File (s : Region)
#
# type error ReadError = ReadError String
#
# service Files =
#   open_scoped : forall s. String ->{Scope s;} File s
#   read_all : forall s. File s ->{; ReadError} String
#
read_later : String ->{Files;} (Unit ->{Files; ReadError} String)
let read_later path = scope {
  let file = Files.open_scoped path
  fun () -> Files.read_all file
}
```

The closure leaves the `scope`, and the row of its type does not keep `s` live:

```text
FLICK-G002: this uses region `s`, which the expected type `Unit ->{Files; ReadError} String` does not keep live: a function or a task uses a region only while the region is live
```

## What the checker asks

- **A runtime stage of a signature says what it uses.** Read a signature the way its caller sees it: each runtime stage needs `Live` of what the parameters before it reach. `read_all : File s ->{; ReadError} String` therefore needs `s` live where it is called, and so does the stage inside a function that returns a closure over its parameter.
- **A qualifier is what using the value needs.** Calling a function `(τ₁ →⋯)^C`, or awaiting a task `(Eff ⋯)^C`, needs `Live C`. In this mode a qualifier no longer says what the value holds: a closure `(Unit ->{} Unit)^{s}` is one whose use needs `s` live.
- **A `scope` is where its region is live.** `Scope s` in a row keeps `s` live — the two are the same row atoms in the core, where `Row.scope s` is `Row.ofCaps s.reach` — and the end of `scope s` discharges `Live s`. A row that uses a region and does not keep it live is `FLICK-G002`.
- **Work an owner keeps outlives the owner.** A child (`Fiber.spawn`), a finalizer, a listener or a group runs until its owner closes, so each region it needs live must end no earlier (`Fiber.Outlives`). Otherwise `FLICK-G003`.
- **An acquisition still needs its owner.** `Live s` gives no `Scope s`: a function that acquires into a region names `Scope s`, as in the default discipline (`FLICK-G004`).

## Errors

| Code | Reported when |
| - | - |
| `FLICK-G001` | a value holds what its bound does not allow; the discipline changes what counts as held |
| `FLICK-G002` | a value uses a region that the bound there does not keep live, or a value would leave its `scope` with a row that needs the region live |
| `FLICK-G003` | work an owner keeps needs a region that may end before its owner |
| `FLICK-G004` | an acquisition needs a `Scope s` that the signature or the place does not give, as in the default discipline |

The second form of `FLICK-G002` is new: in the default discipline a value that keeps a region may not leave its `scope` at all, and in this one it may, provided no row of it needs the region live.

## The two disciplines side by side

| Program | `"captures"` | `"live"` |
| - | - | - |
| a task or a closure holds a handle past its scope and never uses it | `FLICK-G001` | accepted |
| a closure reads a handle after its scope | `FLICK-G001` | `FLICK-G002` |
| a helper whose row has `Scope s` uses what it acquires into `s` | accepted | accepted |
| a child of `outer` uses what an inner `scope` opened | `FLICK-G003` | `FLICK-G003` |
| a function over `File inner` where one over `File outer` is expected | `FLICK-T008` | `FLICK-T008` |

## Compared with Rust

| Rust | Flick |
| - | - |
| a lifetime `'a` | a region: the one a `scope` opens, or a parameter `forall s` |
| `T: 'a` | `Fiber.Outlives c s` for the capture row `c` of a value, or of the work an owner keeps; the checker decides it, and a program declares no instance |
| `fn(&'lt ())` | `File inner -> Unit` |
| `for<'r> fn(&'r ())` | `forall r. File r -> Unit` |

Rust decides `T: 'a` syntactically, as RFC 1214 made it: every lifetime that the type mentions or hides must outlive `'a`, including the argument of a function pointer that the value never calls. The comment in rustc's own `outlives` tests calls that rule stricter than it needs to be — the function pointer is itself `'static`, so it could be bivariant in its argument — and says why it is kept: the rule is purely syntactic.

The default discipline of Flick is that rule: the reach of a function type counts the regions of its parameters and of its result, so `fn(&'lt ())` is no type free of `'lt`, and `File inner -> Unit` is a function that may not leave the scope of `inner`. `"live"` is the reading the comment prefers: holding such a type uses nothing, so `File inner -> Unit` may leave. What makes it sound is not variance but a proof of the live judgment: `T4'_live` in `formal/Formal/Live/T4Live.lean` shows that a program typed this way reaches no step that needs a region after its `scope` has closed, and no operation answered by a handler of a closed region. The compiler is compared with the model by tests, not proved equal to it.

Rust accepts `outlives::<'static, _>((|_| ()) as fn(&'lt ()))` because an argument of type `&'static ()` fits a parameter of `&'lt ()` (contravariance), so `T` is inferred as `fn(&'static ())`. Flick has no subtyping between regions: `File inner` is no `File outer` (`FLICK-T008`). What it has instead is a region parameter, `forall r. File r -> Unit`, which both disciplines accept.

## What it does not check yet

- **Components.** A row with `Setup s` keeps nothing live, and `Setup` is a service that any program can provide, so the checker cannot take it for an open region. `Ui.on_mount`, `Ui.watch` and `Atom.value` inside a component's setup need `Live` of the instance's region, which their closed rows do not keep, so such a component is `FLICK-G002`. Components are checked under `"captures"` until a rule pins `Setup` to the setup of its region.
- **A use through a pure operation.** A lazy `for` sequence and the callback of a background channel reach the region they closed over only through a pure operation, and a pure stage carries no `Live`: both are accepted under `"live"` although they use the region late. The default discipline rejects them.
- **`std`.** It cannot be checked as a project under `"live"`: its `@lang` block is `FLICK-R016`. The built-in modules are checked under `"captures"`.
