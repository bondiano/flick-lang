---
title: "Editors"
---

## Language server

`flick lsp --stdio` is the language server. Its stdout carries only the
protocol; logs go to stderr. It runs the same analysis as `flick check` and
`flick build` over the project the document belongs to (the nearest
`flick.toml`), with the open buffers in place of the files.

| Request | Answer |
| - | - |
| diagnostics | exactly those of `flick check`, with their `FLICK-` codes, at UTF-16 positions |
| `textDocument/hover` | the source type: the signature as written for a top-level value (rows, captures, constraints) with its `///` documentation, the inferred type of a local, a parameter or a pattern variable; on `task` and `scope` the type of the whole block, on `ignore` the type it discards (see [keywords.md](../keywords/#in-the-editor)); never a TypeScript helper |
| `textDocument/definition` | across modules; into a built-in module (`flick-builtin:///List.fl`, read-only, its text through `workspace/textDocumentContent`); into an installed package's published `.fl`, else its `.iface` at the export; a companion (`schema T`, `T.parse`, a `sql table`'s table) leads to its type, a class method used at a known instance to that instance's definition |
| `textDocument/references` | every occurrence in the project |
| `textDocument/rename` | the project's own values, types, constructors and locals, in `.fl` sources only; names of built-in modules and packages, service operations and instance methods have no exact mapping and are refused |
| `textDocument/completion` | after `x.` the fields of `x`'s record (open rows too), after `M.` a module's exports or a service's operations, otherwise locals in scope, the module's names, imported names and the Prelude's |
| `textDocument/formatting` | `flick fmt` |

## TypeScript projection

For editors of TypeScript the compiler projects each module to a virtual
`App/Main.flick.ts`: a declaration per export, in the same TypeScript form
as `--emit-dts` (a type without one becomes `unknown` with the reason), and
span maps between the projection and the `.fl` source. A name maps exactly
(the same text on both sides), a type does not; only an exact mapping can
carry a rename across languages. The first line names the fingerprint of
the analysis the projection comes from, which is also the analysis the
executable build compiles. `flick inspect <module> --emit mapping` prints
the projection with its maps as JSON (format version 1).

## Content Mapper

`tools/content-mapper` adapts the projection to typescript-go through the
TypeScript 7 API, verified against TypeScript 7.0.2: it serves the
projections as a virtual file system and maps positions both ways, so
TypeScript answers what it sees of a Flick module. It is optional: `flick
doctor` reports it, and the Flick checker never depends on it.
