---
title: "Modules and names"
---

One `.fl` file is one module (see [The Flick compiler and Content Mapper integration](../../spec/11-compiler/)). The module header names it and lists
what it exports; imports bring other modules' exports into scope.

```flick
module App.Users exposing (User, load_user)

import App.Domain exposing (User, UserId)
import Effect.Log as Log
```

## Module names and files

A module name is a dot-separated list of capitalized names. In a project
(see [project.md](../project/)) the path below the source root is the module
name: `App.Users` lives in `src/App/Users.fl`. The test root is a second set
of modules with the same rule.

| Situation | Diagnostic |
| - | - |
| `src/App/Users.fl` declares `module App.User` | `FLICK-R001`, with a fix that renames the module |
| a directory or file name is not capitalized (`src/app/users.fl`) | `FLICK-R001` |
| two files declare one module name, e.g. `src/App/Users.fl` and `test/App/Users.fl` | `FLICK-R002` on the later one, sources before tests, then by path |
| a local module and an installed package both provide a module | `FLICK-R002` |

A file without a header takes its name from its path. A file outside the
project roots, or checked without a project, names its own root: `x/App/Main.fl`
declaring `App.Main` imports `App.Util` from `x/App/Util.fl`.

## Imports

| Form | Brings into scope |
| - | - |
| `import App.Domain` | qualified names `App.Domain.name` |
| `import App.Domain as D` | qualified names `D.name` |
| `import App.Domain exposing (User, load)` | `User`, `load` unqualified, plus qualified names |
| `import App.Domain exposing (Shape(..))` | the type `Shape` and its exported constructors |
| `import App.Domain exposing (..)` | every export of `App.Domain` |

A source module imports source modules; a test module imports source and test
modules. Otherwise the module comes from an installed package (see
[project.md](../project/#packages)).

| Situation | Diagnostic |
| - | - |
| no file and no package provides the module | `FLICK-R003`, with where a local module goes and `flick add` |
| a dependency in `package.json` is not installed | `FLICK-P009`, with `flick install` (exit code `2`) |
| `exposing (name)` or `M.name` for a name the module does not export | `FLICK-R004` |
| `exposing (Type(..))` for a type exported without constructors, `exposing (Ctor)` or `M.Ctor` for a constructor that is not exported | `FLICK-R005` |
| modules import each other in a cycle | `FLICK-R006`, listing the chain, once per cycle |

Every error points at the import (or the qualified name) that caused it.

## Exports

The header's `exposing` list is the whole interface of the module. A module
without `exposing` exports nothing.

| Item | Exports |
| - | - |
| `name` | a value: `let`, `foreign`, a class method |
| `Type` | a type, `error`, `kind`, service or class, without constructors or members |
| `Ctor` | one constructor, when no type has that name |
| `Type(..)` | the type and its constructors; the methods of a class; the members of a `kind` |
| `..` | every definition of the module; constructors of opaque types stay private |

Exporting a type does not export its constructors. An `opaque type` never
exports its representation: `Id(..)` for an opaque `Id` is `FLICK-R011`, as are
`Alias(..)` for a type without constructors and a name the module neither
defines nor imports.

A module may re-export a name it imports. A re-export is the same entity, not
a copy: importing `User` both from `App.Domain` and from a module that
re-exports it is not ambiguous, and a re-exported service is reached
through the re-exporter's path as through its own: `M.S` in a row, `M.S.op`,
`M.S.Impl` and `M.S` as a value. A re-exported `main` makes the module an entry point too, its runner
importing `main` from where it is defined; a re-exported `tests` does not
make it a test module.

```flick
module App.Users exposing (User, load_user)

import App.Domain exposing (User)
```

## Top-level definitions

Within a module a name is defined once per namespace: values, types (types,
errors, kinds and their members, services, classes) and constructors. A
signature and its `let` are one definition; a second definition is
`FLICK-R008`.

Transparent aliases must be acyclic: `type alias A = List A`, or a cycle through
several aliases, is `FLICK-R007`. Recursion goes through a nominal type, which
may refer to itself and to other types of the file:

```flick
type Tree a = Leaf | Node a (Forest a)
type alias Forest a = List (Tree a)
```

## Shadowing and ambiguity

Each rule compares names and module names, never the order of imports, so
reordering imports changes no diagnostic.

1. A local binding (parameter, pattern, `let`, `scope`) shadows every
   top-level and imported name; this is allowed.
2. A top-level definition shadows names from `exposing (..)` imports silently,
   but a name imported by name in `exposing (name)` is `FLICK-R009`: rename the
   definition or drop the import.
3. Two imports that expose one name by name as different entities are
   `FLICK-R010`, reported on the import of the module whose name sorts last.
4. A name brought in by two `exposing (..)` imports as different entities is
   `FLICK-R010` where it is used unqualified; unused, it is no error.
5. Two imports under one qualifier (`import A as M`, `import B as M`) are
   `FLICK-R010`.

```flick
import App.Users as Users exposing (..)
import App.Orders as Orders exposing (..)

let first = Users.find
```

Names inside `sql { }` belong to the query and are resolved by SQL elaboration.
Names that nothing defines are reported by the type checker, which knows the
prelude.
