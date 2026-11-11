---
title: "`.iface` — native module interface"
---

An `.iface` file describes what one module exposes, in the form the checker
needs. Ordinary imports use those checked types directly. Static parameterized modules also
carry a checked template that is instantiated for its module arguments. A saved
native package can instantiate it without the original checkout (see
[Compiler architecture](../../contributing/implementation-decisions/#compiler-architecture)).
It is versioned JSON. `.d.ts` is only a projection for TypeScript; `.iface` is the source of truth for Flick importers.

The current schema is `iface-version` 15. It includes module visibility,
`type module` contracts, nested static modules, higher-order signatures, the
`exists` type form and implicit conversion instances. Every schema change
bumps the version; a compiler rejects any other version with `FLICK-P010`. The simplified example below shows the envelope;
use the current compiler to write an interface for a new package.

## Example

```json
{
  "iface-version": 15,
  "compiler": "0.1.0",
  "module": "App.Shapes",
  "exports": [
    { "module": "App.Shapes", "namespace": "type", "name": "Shape", "decl": "adt" },
    { "module": "App.Shapes", "namespace": "constructor", "name": "Circle", "decl": "constructor", "parent": "Shape" },
    { "module": "App.Shapes", "namespace": "type", "name": "Id", "decl": "opaque" },
    { "module": "App.Domain", "namespace": "type", "name": "User", "decl": "alias" },
    { "module": "App.Shapes", "namespace": "value", "name": "area", "decl": "let" }
  ],
  "hidden": [
    { "name": "Square", "parent": "Shape" }
  ]
}
```

Rebuild dependencies written with an older schema; changing the version
number by hand cannot recover missing capture information. A template's
`capture.scopes` records each defining module and source-prefix boundary,
with the counts of bindings and inherited imports in scope there. Offline
instantiation checks the saved prefix and keeps its original entity owners;
later arguments do not enter that lexical environment.

| Field | Meaning |
| - | - |
| `iface-version` | schema version; must equal the compiler's |
| `compiler` | version of the compiler that wrote the file; must equal the reader's |
| `module` | full name of the described module |
| `modules` | exported static module members, each with its own interface; omitted when empty |
| `parameterized` | checked higher-order signature of a static parameterized module; arrows retain scoped binder identities, parameter signatures, result signatures and atomic group sizes; omitted for structures |
| `signature_explicit` | `true` when the parameterized module result comes from an explicit declaration or ascription; public aliases cannot inherit a private inferred result |
| `member_of` | owner namespace and member name for a generated static member; external access uses the checked parent view |
| `template` | checked static body (`source`) and its lexical `capture`: module bindings, signature origins, partial arguments with the caller's namespace access, lexical namespaces, inherited-import count and definition package; omitted for ordinary structures |
| `private` | `true` for a module importable only within its package; omitted otherwise |
| `exports` | the module's interface, in the order of its `exposing` list |
| `hidden` | constructors of exported types that are not exported, so an import of one gets a precise `FLICK-R005`; omitted when empty |
| `docs` | the `///` comment of each export the module declares, by name, which its `.d.ts` carries as JSDoc; omitted when empty |
| `instances` | the instances of the module and of every module it depends on, except built-in ones; omitted when empty |
| `abi` | how the compiled code represents what the module defines, see [ABI](#abi); written by `flick build`, absent from `flick check` |
| `dependencies` | the fingerprint of each imported module's interface the module was built against; omitted when empty |
| `source` | the module's source, relative to the root of its package, so editors find declarations in a published package; written by `flick build` |
| `targets` | the targets `flick build` compiled the module for, `["js"]` today; a project with `emit = "go"` that imports a module without `"go"` gets `FLICK-P030`; omitted when no build wrote the file |

A native saved package keeps both these templates and the portable IR of their
ordinary dependencies. It can instantiate a factory for a new argument and build
JS or Go with `--offline` after the original `.fl` files have been removed. The
package manifest, lock and saved artifacts must still be available. Path
packages remain keyed by their current source files; an immutable fetched
package is keyed by its locked checksum. A changed original file is reported
as `FLICK-P034` during a build instead of mixing it with saved code.

Compiler-only builds without `native-project` or `build-artifacts` omit persistent
templates from serialization. They still elaborate static module expressions
from available source; disk package caches and published template metadata belong
to those capabilities.

Captures retain access to the definition package's private implementation
modules. That access belongs to the stored body; importing its public factory
does not grant clients access to those private paths. A template body change
invalidates its fingerprint, while API comparison uses the public signature and
nested members rather than the implementation body.

Each export names an **entity** by `module`, `namespace` and `name`. `module`
is where the entity is *defined*: a re-export keeps the defining module, which
is how importers see that two names are the same entity (above, `User` is
re-exported from `App.Domain`).

| `namespace` | Entities |
| - | - |
| `value` | `let`, `foreign`, class methods |
| `type` | types, `error`, `kind` and its members, services, classes |
| `constructor` | ADT constructors, the constructor of an `error` |

| `decl` | Declaration | `parent` |
| - | - | - |
| `let` | `let`, or a signature whose definition is checked later | — |
| `foreign` | `foreign` | — |
| `method` | a method signature of a class | the class |
| `adt` | `type T = A \| B` | — |
| `alias` | `type alias T = …` | — |
| `opaque` | `opaque type` | — |
| `refined` | `refined type` | — |
| `abstract` | `type T` without a definition | — |
| `error` | `type error E …` | — |
| `kind` | `kind K = …` | — |
| `kind-member` | a member of a `kind` | the kind |
| `service` | `service` | — |
| `class` | `class` | — |
| `constructor` | a constructor | its type |

`T(..)` in an import exposes the exports whose `parent` is `T`.

An exported type also exports its companion values as `value` entities named
`T$member`, which no source name can spell (`T$table` of a `sql table`): `T$schema` of a type deriving
`Schema`, `T$parse` and `T$value` of a refined type, and `T$json` of a
refined type with a schema. `T.schema` and `T.parse` name them. An export
of a declaration of the module has `at`, the UTF-8 byte range of its name in
`source`. A class's
`definition` lists in `defaults` the methods with a default definition.

### Types and kinds

An importer's checker takes the types of a dependency from its interface
only, never from its source. A module with syntax errors is not type-checked;
its exports carry no types, and importers accept any use of them.

| Field | On | Contents |
| - | - | - |
| `type` | values, constructors | the full type: `forall` with the kinds of its variables, constraints, effect arrows and rows |
| `definition` | types, errors, services, classes, kinds | `kind` and `form`, see below |

```json
{ "module": "App.Shapes", "namespace": "value", "name": "wrap", "decl": "let",
  "type": { "forall": {
    "vars": [ { "name": "f", "kind": { "arrow": ["type", "type"] } }, { "name": "a", "kind": "type" } ],
    "body": { "fun": {
      "param": { "app": { "head": { "var": "f" }, "arg": { "var": "a" } } },
      "result": { "record": { "entries": [ ["value", { "app": { "head": { "var": "f" }, "arg": { "var": "a" } } }] ] } }
    } }
  } } }
```

Types are tagged objects; a row lists `entries` sorted by label and an
optional `tail`. Entries of a record row are labeled with field names, those
of a service or error row with the element's `Module.Name`, those of a
capture row with `^` and the region variable.

| Type | JSON |
| - | - |
| `Float` | `{ "con": { "module": "Prelude", "name": "Float" } }` |
| type variable `a` | `{ "var": "a" }` |
| `F a` | `{ "app": { "head": F, "arg": a } }` |
| `a -> b` | `{ "fun": { "param": a, "result": b } }` |
| `a ->{r; e} b` | `{ "fun": { "param": a, "effects": { "services": r, "errors": e }, "result": b } }` |
| `(a, b)` | `{ "tuple": [a, b] }` |
| `{ x : T \| r }` | `{ "record": { "entries": [["x", T]], "tail": r } }` |
| `{Users \| r}`, `none` | `{ "row": { "entries": [["App.Users.Users", …]], "tail": r } }`, `{ "row": { "entries": [] } }` |
| `r1 + r2` | `{ "union": [r1, r2] }` |
| hidden qualifier `T^{s}` | `{ "captured": { "ty": T, "captures": { "row": { "entries": [["^s", { "var": "s" }]] } }, "value": false } }` |
| whole generic bound `a^c` | `{ "bounded": { "ty": { "var": "a" }, "captures": { "var": "c" } } }` |
| `forall a. C a => T` | `{ "forall": { "vars": [{ "name": "a", "kind": K }], "constraints": [C a], "body": T } }` |

`bounded` refines a complete generic value's capture bound while preserving
the underlying type and its qualifications. `captured` retains a hidden or
stored qualifier. Its required `value` flag is `false` for an explicit or
stored qualification and `true` when classification is deferred to a value
boundary, for example while a higher-kinded head is unresolved. Importers
must preserve both tags and the flag: replacing them with one qualifier or
dropping a zero row can lose capture soundness. See [Regions and captures](../regions/).

Kinds are `"type"`, `"service-row"`, `"error-row"`, `"capture-row"`,
`"symbol"`, `"region"`, `"constraint"`, `"service"` (an element of a service
row), `{ "row": K }`, `{ "named": { "module": …, "name": … } }` for a `kind`
declaration and `{ "arrow": [K1, K2] }`.

| `form` | Fields | Declaration |
| - | - | - |
| `nominal` | — | built-in, `type T`, `opaque type`, `refined type`; also an opaque ADT, whose constructors stay hidden |
| `adt` | `constructors` | variants, every constructor including unexported ones (for exhaustiveness) |
| `error` | — | `type error E …`; its constructor is `E` |
| `alias` | `params`, `body` | transparent alias |
| `service` | `params`, `operations` (`name`, `type`) | `service`; `operations` hold the types as declared, without the service itself, and an importer derives `S.Impl` from them |
| `class` | `params`, `supers`, `fundeps` (`from`, `to`: parameter positions), `methods` | `class`; methods are `value` exports, their type starting with the class constraint |
| `kind` | — | `kind K = …`; `kind` is the kind it names |
| `kind-member` | — | a member of a `kind` |

Transparent aliases are expanded in every exported type, so an importer never
needs the alias of a module it does not import. An opaque type is `nominal`
outside its module: importers cannot see its representation.

### Instances

```json
{ "module": "App.Shapes", "name": "Show (List a)",
  "vars": [ { "name": "a", "kind": "type" } ],
  "context": [ { "app": { "head": { "con": { "module": "App.Shapes", "name": "Show" } }, "arg": { "var": "a" } } } ],
  "head": { "app": { "head": { "con": { "module": "App.Shapes", "name": "Show" } },
                     "arg": { "app": { "head": { "con": { "module": "Prelude", "name": "List" } }, "arg": { "var": "a" } } } } } }
```

`module` declares the instance and `name` is its head as written; `head` and
`context` are over `vars`, with transparent aliases expanded. An importer
checks its own instances against these for overlap and functional
dependencies, and resolves constraints with them.

`implicit: true` marks an `@implicit` conversion instance. The field defaults
to `false` and is omitted when false. Importers use it when choosing a unique
one-step conversion between ground nominal types.

### ABI

```json
"abi": {
  "data": [ { "type": "Shape", "constructors": [
    { "name": "Circle", "tag": "@acme/shapes/App.Shapes.Shape.Circle", "arity": 1 } ] } ],
  "dictionaries": [ { "instance": "Show (List a)", "export": "$i_Show_List_a_19082212",
    "context": 1, "methods": ["show"], "supers": [] } ],
  "values": [ { "name": "area", "stages": ".", "dicts": 0, "partial": false } ]
}
```

| Field | Contents |
| - | - |
| `data` | each type and error the module declares, with its constructors: `id` is the qualified id (package, module, type, constructor), `tag` the `_tag` of their values, the name and a hash of the id (none for an `unboxed` or `untagged` type, whose values are their field), `arity` the fields `_0` … |
| `dictionaries` | each instance the module declares: the JS `export` of its dictionary, how many context dictionaries it takes, its `methods` and the superclasses it has a `$Class` getter for |
| `values` | each exported value: `stages` has one character per parameter of its definition, `.` for a pure and `!` for a runtime arrow; `dicts` counts the dictionaries it takes first; `partial` is `false` only when applying it to all its parameters certainly neither throws nor diverges |

[codegen.md](../codegen/) describes the representation these fields name.

### Fingerprints

The fingerprint of an interface is a 64-bit FNV-1a hash, as 16 hex digits,
of its JSON without `dependencies`, `source`, `docs` and the `at` of exports:
moving or documenting a declaration changes nothing for importers. Equal fingerprints mean that code
compiled against one interface is valid against the other. `dependencies`
records the fingerprint of every imported module's interface at build time.

## Location in a package

`flick build` writes `A/B.iface` next to `A/B.js` in the output directory.
Dependents compile a Flick library from its sources or restore its saved
checked package, including static templates (see
[project.md](../project/#flick-dependencies)).
