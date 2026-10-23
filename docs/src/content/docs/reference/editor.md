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
| `textDocument/hover` | source signatures and `///` documentation for public and private values, at declarations and uses; module documentation on qualifiers such as `Sql`; inferred locals and values, SQL aliases, columns and parameters, on `task` and `scope` the type of the whole block, on `ignore` the type it discards (see [keywords.md](../keywords/#in-the-editor)); declarations distinguish `type error` from `service` |
| `textDocument/definition` | across modules; module qualifiers lead to their module header, members to their declaration; built-ins open as real read-only `.fl` files; installed packages use their published `.fl`, else `.iface`; companions lead to their type, class methods at a known instance to that instance |
| `textDocument/references` | every occurrence in the project |
| `textDocument/rename` | the project's own values, types, constructors and locals, in `.fl` sources only; names of built-in modules and packages, service operations and instance methods have no exact mapping and are refused |
| `textDocument/completion` | record fields after `x.`, module exports or service operations after `M.`, otherwise names in scope; SQL blocks offer tables, aliases, columns, parameters, clauses and functions; `sql table` columns offer SQL types and modifiers |
| `textDocument/formatting` | `flick fmt` |

Hover resolves source bindings, not just the public export list. Private
helpers such as a view or an error reporter show their written signatures
and documentation without making them public. The `ignore` keyword shows its
discard type at that use (for example `Never -> Unit` around `Ui.run`);
discarding a result does not discard the expression's effects or errors.
The [Gleam language server reference](https://gleam.run/documentation/language-server-reference/)
is the behavior reference for symbol hover and navigation, not a claim of
complete feature parity.

SQL column hover and completion preserve nullability: a column from a
`left_join` reads as `Option`, without adding a second `Option` to an already
nullable column. Nested queries have their own table aliases.

Built-in definitions use ordinary `file://` URIs in both Zed and VS Code.
The server materializes its embedded sources under `flick-lsp` in the system
temporary directory, keyed by content and read-only. Opening them checks
them as library sources, without adding them to the application's project.
Both `textDocument/prepareRename` and `textDocument/rename` reject these
read-only documents rather than returning edits to the cached source.

## Zed

The extension first uses `lsp.flick.binary.path`, if set, then `flick` on
the worktree's `PATH`. Otherwise it downloads the GitHub release tagged
`v<extension version>` for the current platform into its work directory.
An installed copy is reused; if the release cannot be reached, the newest
previously downloaded version is used.

To use a local checkout, run `cargo build -p flick` there and add this to
Zed's settings, replacing the path with your absolute checkout path:

```json
{
  "lsp": {
    "flick": {
      "binary": {
        "path": "/absolute/path/to/flick-lang/target/debug/flick",
        "arguments": ["lsp", "--stdio"]
      }
    }
  }
}
```

An explicit path bypasses release lookup and download, including when the
repository is private or no matching release exists. An invalid explicit
path fails to start rather than silently falling back to another binary.
`binary.arguments` replaces the default `["lsp", "--stdio"]`;
`binary.env` overrides variables from the worktree's shell environment.

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
