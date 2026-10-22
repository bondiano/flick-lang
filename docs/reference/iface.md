# `.iface` — native module interface

An `.iface` file describes what one module exposes, in the form the checker
needs, so an importer never re-reads, re-parses or re-checks the module's
source (§11). It is versioned JSON. `.d.ts` is only a projection for
TypeScript; `.iface` is the source of truth for Flick importers.

Until 0.1.0 the schema grows with the compiler: every change bumps
`iface-version`, and a compiler rejects any version but its own with
`FLICK-P010`. Release 0.1.0 fixes version 1.

## Version 0

```json
{
  "iface-version": 0,
  "language": "0.7",
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

| Field | Meaning |
| - | - |
| `iface-version` | schema version; must equal the compiler's |
| `language` | Flick language version of the compiler that wrote the file; must equal the reader's |
| `module` | full name of the described module |
| `exports` | the module's interface, in the order of its `exposing` list |
| `hidden` | constructors of exported types that are not exported, so an import of one gets a precise `FLICK-R005`; omitted when empty |

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
| `method` | `val` of a class | the class |
| `adt` | `type T = A \| B` | — |
| `alias` | transparent `type T = …` | — |
| `opaque` | `opaque type` | — |
| `refined` | `refined type` | — |
| `abstract` | `type T` without a definition | — |
| `error` | `error E of …` | — |
| `kind` | `kind K = …` | — |
| `kind-member` | a member of a `kind` | the kind |
| `service` | `service` | — |
| `class` | `class` | — |
| `constructor` | a constructor | its type |

`T(..)` in an import exposes the exports whose `parent` is `T`.

## Location in a package

A package names the directory of its interfaces in `package.json`, and module
`A.B` is described by `<iface-dir>/A/B.iface`:

```json
{ "name": "@acme/text", "flick": { "iface-dir": "iface" } }
```

`flick build` and `flick pack` write both; nobody edits them by hand.

## Planned additions

Each addition is a new `iface-version`, added by the phase that produces the
information:

| Addition | Contents |
| - | - |
| types and kinds | the full type of every value (signatures with `forall`, constraints, effect arrows and rows), type parameters and their kinds, alias expansions, constructor payloads, opaque types without their representation |
| classes | class heads, superclasses, functional dependencies, instance heads, the dictionaries an instance selects |
| captures | capture and region constraints of public functions, never erased |
| ABI and summaries | data ABI of each type, arity, purity and partiality for cross-module optimization |
| fingerprints | a fingerprint of the interface and of each dependency's interface, for incremental builds and `pack` |
