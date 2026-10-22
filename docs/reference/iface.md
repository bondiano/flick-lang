# `.iface` — native module interface

An `.iface` file describes what one module exposes, in the form the checker
needs, so an importer never re-reads, re-parses or re-checks the module's
source (§11). It is versioned JSON. `.d.ts` is only a projection for
TypeScript; `.iface` is the source of truth for Flick importers.

Until 0.1.0 the schema grows with the compiler: every change bumps
`iface-version`, and a compiler rejects any version but its own with
`FLICK-P010`. Release 0.1.0 freezes the version it ships with.

## Version 1

```json
{
  "iface-version": 1,
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
of a service or error row with the element's `Module.Name`.

| Type | JSON |
| - | - |
| `Number` | `{ "con": { "module": "Prelude", "name": "Number" } }` |
| type variable `a` | `{ "var": "a" }` |
| `F a` | `{ "app": { "head": F, "arg": a } }` |
| `a -> b` | `{ "fun": { "param": a, "result": b } }` |
| `a ->{r; e} b` | `{ "fun": { "param": a, "effects": { "services": r, "errors": e }, "result": b } }` |
| `(a, b)` | `{ "tuple": [a, b] }` |
| `{ x : T \| r }` | `{ "record": { "entries": [["x", T]], "tail": r } }` |
| `{Users \| r}`, `none` | `{ "row": { "entries": [["App.Users.Users", …]], "tail": r } }`, `{ "row": { "entries": [] } }` |
| `r1 + r2` | `{ "union": [r1, r2] }` |
| `forall a. C a => T` | `{ "forall": { "vars": [{ "name": "a", "kind": K }], "constraints": [C a], "body": T } }` |

Kinds are `"type"`, `"service-row"`, `"error-row"`, `"capture-row"`,
`"symbol"`, `"region"`, `"constraint"`, `"service"` (an element of a service
row), `{ "row": K }`, `{ "named": { "module": …, "name": … } }` for a `kind`
declaration and `{ "arrow": [K1, K2] }`.

| `form` | Fields | Declaration |
| - | - | - |
| `nominal` | — | built-in, `type T`, `opaque type`, `refined type`; also an opaque ADT, whose constructors stay hidden |
| `adt` | `constructors` | variants, every constructor including unexported ones (for exhaustiveness) |
| `error` | — | `error E of …`; its constructor is `E` |
| `alias` | `params`, `body` | transparent alias |
| `service` | `params`, `operations` (`name`, `type`) | `service`; `operations` hold the types as declared, without the service itself, and an importer derives `S.Impl` from them |
| `class` | — | `class`; methods are `value` exports |
| `kind` | — | `kind K = …`; `kind` is the kind it names |
| `kind-member` | — | a member of a `kind` |

Transparent aliases are expanded in every exported type, so an importer never
needs the alias of a module it does not import. An opaque type is `nominal`
outside its module: importers cannot see its representation.

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
| classes | class heads, superclasses, functional dependencies, instance heads, the dictionaries an instance selects |
| captures | capture and region constraints of public functions, never erased |
| ABI and summaries | data ABI of each type, arity, purity and partiality for cross-module optimization |
| fingerprints | a fingerprint of the interface and of each dependency's interface, for incremental builds and `pack` |
