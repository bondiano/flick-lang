---
title: "Modules and names"
---

One `.fl` file is one module. The module header names it and lists
what it exports; imports bring other modules' exports into scope.

## Inline modules

An inline module groups declarations in the same source file. Its body may
declare values, types, classes, instances, services, effects, contracts and
other inline modules. Indent the body under `=`:

```flick
module App.Article exposing (answer)

type Key = Key Int
let seed = 41

module Comments exposing (value) =
  value : Key
  let value = Key (seed + 1)

answer : Key
let answer = Comments.value
```

`Comments` sees the parent's earlier declarations, including private `Key`
and `seed`. The parent uses only members in the child's `exposing` list,
qualified as `Comments.value`; without `exposing`, the child exports no
members to the parent. Child declarations have their own namespace, so a
type or class in `Comments` is distinct from one with the same name in the
parent. A child may import modules within its body; those imports stay in
the child.

An inline child is private to its source file. `App.Article.Comments` is not
an import path, even for another file in the same package. Keep the child
out of the parent's `exposing` list. The compiler emits the file as one
runtime module, preserving initialization order across the boundary. An
attempt to import the inline path reports `FLICK-R018` at the import and
points to its declaration.

Classes and services declared in a child keep their own nominal identities.
A `type module` contract may be declared inside the child. To pack the whole
child as `Module Sig`, a contract outside it can require those identities
explicitly with `class Describe = Child.Describe` and
`service Greeter = Child.Greeter`; `pack Child as Sig` then checks its exported
members. See [module contracts](#module-contracts-and-parameterized-modules) for the
rules on abstract requirements and dictionaries.

| Situation | Diagnostic |
| - | - |
| two inline children have the same name, or a child occupies a file module's path | `FLICK-R002` at the conflicting declaration |
| the parent refers to a member absent from the child's `exposing` list | `FLICK-R004` at the qualified name |
| another file imports the child or a deeper path through it | `FLICK-R018` at the import, with the inline declaration as a related location |
| a named application of a parameterized module is declared inside the child | `FLICK-R019` at the application name; declare the result in a file module |

Prefix the header with `private` to make a file module available only to
source and test files of its package:

```flick
private module App.Internal exposing (helper)
# helper : Int -> Int
# let helper x = x
```

An importer in another package gets `FLICK-R017` at its import path. A public
module of the same package can re-export names from a private module; clients
import that facade. A private module remains private when a dependency is
compiled from source or loaded from the package cache.

```flick
module App.Users exposing (User, load_user)

import App.Domain exposing (User, UserId)
import Effect.Log as Log
#
# load_user : User -> User
# let load_user user = user
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
#
# load_user : User -> User
# let load_user user = user
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

Functions that call themselves or each other are declared with `let rec` or
in one `rec { … }` group (see [Recursion](../types/#recursion)).

<div id="module-contracts-and-named-functors"></div>

## Module contracts and parameterized modules

`type module` declares a structural contract for a module. A required
`type Key` is abstract to users of the contract; `type Key = String` requires
that equality. A concrete module may export additional members.

```text
type module Source =
  type Key
  value : Key

module App.Make(S : App.Contracts.Source) : Answer
// App/Make.fl declares: type module Answer = answer : Int
// App/Make.fl uses S.Key and S.value

module App.Parent exposing (Chosen)
module Chosen = App.Make(App.Argument)
// App.Parent.Chosen can be imported from another file
```

A parameterized module takes a named module known during compilation. Each `module Chosen
= App.Make(App.Argument)` declaration creates a distinct result module and
distinct identities for its abstract result types. Importing that result in
multiple files preserves its identity. The parent must export `Chosen` for
other modules to import `App.Parent.Chosen`. The parameterized module body sees only the
members and equalities listed in its argument contract.

For example, `CaseInsensitiveSet.Set` and `ExactSet.Set` are distinct even
when both policies use `String` keys. Two files importing the same
`CaseInsensitiveSet` result can exchange sets. Passing one of its sets to an
`ExactSet` operation reports `FLICK-T003` with both full type paths; convert
explicitly through exported operations such as `to_list` and `from_list` when
the change of policy is intentional.

Contracts may require classes and services. Their identities are nominal:
the same method or operation shape does not make two declarations identical.
Use `class Eq = Prelude.Eq` or `service Clock = Time.Clock` to require an
existing identity. An abstract class or service requirement is available to
a parameterized module, but `pack` requires the manifest form.

```text
type module Policy =
  type Key = String
  class Eq = Prelude.Eq
  equality : Eq.Dict Key
  instance Eq Key from equality

let equality = Eq.dictionary { eq = fun _ _ -> false }
let hashing = Hash.dictionary {
  Eq = equality
  hash = fun _ -> 0
}
```

`instance Eq Key from equality` uses that named dictionary within a parameterized module
body or an `unpack` body. An explicit given dictionary takes precedence;
otherwise the local dictionary takes precedence over a global instance.
The requirement does not declare a global instance. If two arguments in one
parameter group provide the same constraint, `FLICK-T066` reports the ambiguity.
A later curried group introduces a nested scope: its local dictionary takes
precedence over a captured dictionary. A dictionary for a class with a superclass must include its named superclass
field, as `Hash.dictionary` does above.

A parameterized module can also require a class whose identity is supplied by its
argument. The contract can name its dictionary type and method. In the
parameterized module, `K.equal` resolves through the dictionary named by `instance`:

```text
type module EqualityPolicy =
  class Equality a =
    equal : a -> a -> Bool
  equality : Equality.Dict String
  instance Equality String from equality

module App.Make(K : App.Contracts.EqualityPolicy) : Answer
answer : Bool
let answer = K.equal "A" "A"

type module Answer =
  answer : Bool
```

### Generative result types

This example applies the same parameterized module to the same argument twice. Each block
is a complete file; save it at the path shown in its title. The parameterized module
exports `Token` without its constructor, and each application creates a
distinct token type.

```text title="src/App/Contracts.fl"
module App.Contracts exposing (Source)

type module Source =
  seed : String
```

```text title="src/App/Argument.fl"
module App.Argument exposing (seed)

seed : String
let seed = "Ada"
```

```text title="src/App/Make.fl"
module App.Make(S : App.Contracts.Source) : Result

type Token = Token String

value : Token
let value = Token S.seed

type module Result =
  type Token
  value : Token
```

```text title="src/App/Instances.fl"
module App.Instances exposing (A, B)

module A = App.Make(App.Argument)
module B = App.Make(App.Argument)
```

Both files below import the **same result**, `App.Instances.A`, so they can
exchange its tokens. `flick check src/App/Consumer.fl` succeeds: importing
`A` again does not create a new type.

```text title="src/App/Producer.fl"
module App.Producer exposing (value)

import App.Instances.A as A

value : A.Token
let value = A.value
```

```text title="src/App/Consumer.fl"
module App.Consumer exposing (value)

import App.Instances.A as A
import App.Producer

value : A.Token
let value = App.Producer.value
```

The second application, `B`, has its own type. Add this file and run
`flick check src/App/Bad.fl`: it reports `FLICK-T003`, with
`App.Instances.B.Token` expected and `App.Instances.A.Token` found.

```text title="src/App/Bad.fl"
module App.Bad exposing (value)

import App.Instances.A as A
import App.Instances.B as B

value : B.Token
let value = A.value
```

The declarations `module A = …` and `module B = …` create the identities;
imports preserve them. The documentation tests check these files together
and require the last file to fail with `FLICK-T003`.

## Higher-order module signatures

Static expansion compares the active template and its module arguments, so a finite higher-order callback may revisit a template with different arguments. Repeating the same active application reports `FLICK-R006`. Expansion also stops at 64 active applications with `FLICK-R006`, including recursive definitions that keep producing fresh arguments. This is a compiler resource bound, not a termination proof.

Public parameterized modules declare their result signature (`FLICK-T067`). Private and local
parameterized modules may infer it. A public alias may inherit an explicitly advertised
signature or add an ascription. An optional `exposing` list must agree with
the result signature; it cannot request hidden members.
Inference retains the arity and kinds of type members and their transparent
equalities. A class or service declared by a factory body belongs to the named
application; passing or re-exporting an existing declaration keeps its identity.

A module contract may describe a parameterized module. Its named parameter is in scope in
the result signature. `with type` exposes an equality that clients may rely
on; it cannot introduce a missing member or contradict an existing equality.

```flick
# module App.Contracts exposing (..)
type module Input =
  type Key

type module Output =
  type Value
  value : Value

type module Factory =
  (K : Input) -> Output with type Value = K.Key

type module Grouped =
  (A : Input, B : Input) -> Output with type Value = A.Key

type module Curried =
  (A : Input) -> (B : Input) -> Output with type Value = A.Key

type module Namespace =
  module M : Input
  value : M.Key

type module NestedFactory =
  (K : Namespace) -> Output with type Value = K.M.Key

type module WithFactory =
  type Item
  module Make : Input -> Output with type Value = Item

type module Lists =
  type Items a = List a
  empty : forall a. Items a
```

A structure member also binds a module path: `value : M.Key` must use the
same `Key` exported by `M`. Its implementation cannot supply an unrelated
type with a matching name. Projection retains the argument's identities;
sealing a result gives its abstract nested members fresh identities and
updates dependent types throughout the result.

An outer structure's type members are also in scope inside nested signatures.
In `WithFactory`, `Make` returns that structure's `Item`. A nested structure's
own type members shadow outer names. An arrow parameter's members keep their
module qualifier in its result signature.

A group `(A : Input, B : Input)` takes both arguments in one application.
Separate groups `(A : Input)(B : Input)` allow partial application and match
a curried signature. The two forms are distinct contracts. Parameters retain
their own visible members even when they receive the same module. Aliases
preserve type identities and the visibility of their source parameter.
A nested parameterized module inherits the enclosing imports, including their exact
`exposing` lists and imported instances. Aliases used only in its parameter
or result signature remain in scope.

It also captures preceding local signatures, types and values. Later local
declarations are outside that definition's scope. Capturing a value retains
the original binding: two applications that return an enclosing `Layer`
share that layer object, including its memoization identity. Values in the
prefix are initialized before a result that reads them; the factory does not
copy or reevaluate those definitions with the caller's dictionaries.

An ascription such as `module Narrow : Factory = F` checks the advertised
view of `F`, then restricts the alias to `Factory`. It cannot recover members
hidden by the parameter contract, even when the actual argument exports them.
The original `F` retains its own view. The same rule applies to an annotated
application, `module Result : Output = Make(Argument)`.

Recursive static module expansion is rejected with `FLICK-R006`. Finite nested
applications such as `Identity(Identity(Argument))` may reuse the same parameterized module.

An arrow is a static module signature, separate from a value function type.
An implementation may accept fewer requirements and return more members than
the signature requires. A result annotation hides members and type equalities
that the contract does not expose. Runtime `Module Sig` records are flat: they
cannot carry parameterized modules or nested module namespaces (`FLICK-T068`). Apply factories
statically and explicitly flatten the values and type witnesses before packing.

### Repository layers and logging

`examples/hom-repository/` contains a complete checked example. `Make(Database)`
exports operations and a `Layer`; connection acquisition happens inside
`Layer.from_build` when the layer runs. `WithLogging` accepts this factory and
returns an anonymous factory:

```text
module App.WithLogging(Factory : App.Contracts.Factory) : App.Contracts.LoggedFactory =
  module (Database : App.Contracts.Database) -> App.Logged(Factory(Database))
```

The repository and logging contracts explicitly require the same nominal
`Domain.Repo`. The adapter's `get` operation adds `Log.Log` to its service row;
forwarding the layer preserves the service identity and its resource owner.
A caller that provides only the repository layer still needs `Log` and is
rejected with `FLICK-E002`. The example's integration test checks deferred
acquisition, success, failed acquisition, failed initialization, body failure,
interruption and overlapping layer runs, including exactly-once release, on
JS and Go in both profiles.

## Packed modules and existential results

`pack Argument as Source` has type `Module Source`. Use `unpack packed as K
in ...` to open it in a lexical scope. Every opening creates a fresh witness
for abstract types, so a value of `K.Key` cannot silently lose its witness.
When such a value leaves `unpack`, the result is existential; an exported
binding must state its type explicitly:

```text
result : exists K : Box. K.Key
let result = unpack (pack Argument as Box) as K in K.value

answer : Int
let answer = unpack result as K, item in (fun _ -> 42) item
```

`unpack result as K, item in ...` opens both the module witness and its
carried value. Separate openings produce unrelated abstract witnesses.
`Module Sig` has no carried value, so its opening uses `as K in`.
Parameterized module application is static: its argument must be a named module, not a
`Module Sig` value. TypeScript sees an exported existential value as
`unknown`, since it cannot express the fresh witness.

Contract diagnostics distinguish a missing member (`FLICK-T060`), a
mismatched member type or manifest equality (`FLICK-T061`), a wrong nominal
class or service identity (`FLICK-T062`), an abstract nominal requirement in
`pack` (`FLICK-T063`), an escaping or unannotated existential witness
(`FLICK-T064`), and an invalid dictionary requirement (`FLICK-T065`).

## Initialization and entry modules

Loading a module evaluates its top-level values, so they are built purely:
an effectful call there is `FLICK-E001`, a top-level `task` is fine because
its body is deferred (see [Stages](../effects/#stages)), and of foreign
functions only a `pure` one may run at load (see [Modes](../ffi/#modes)).
Importing a module, a library's included, runs nothing else.

An app names its entry modules in `flick.toml`, `entry` or `entries` (see
[Entry points](../project/#entry-points)). Each exports `main`, its own or
re-exported. A pure `main : Unit -> Unit` is called directly; a runtime
`main` takes and returns `Unit`, needs no service and fails only with
`AppError` (`FLICK-P026` otherwise), so it provides its layers and handles
other errors itself.

An entry marked `target = "client"` runs in a browser: nothing its `main`
reaches may need `Server` (`FLICK-E009`, with the path to it), so its bundle
holds no server module and no secret. Such code reaches the server through
a [`server fn`](../server-fn/): the client calls it by name and needs no
service for it, while a server entry serves it with `Server.handler`.
`flick new --template fullstack` writes such a pair, and `flick dev` runs the
server entry and Vite for the page:

```toml
[project]
kind = "app"
runner = "Runner.Node"
entries = { server = "App.Server", web = { module = "App.Web", runner = "Runner.Browser", target = "client" } }
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
