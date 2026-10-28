---
title: "Editors"
---

## Language server

`flick lsp --stdio` is the language server. Its stdout carries only the
protocol; logs go to stderr. It runs the same analysis as `flick check` and
`flick build` over the project the document belongs to (the nearest
`flick.toml`), with the open buffers in place of the files.

Hover and navigation follow unsaved edits in imported open modules, including
standalone files without `flick.toml`. Closing an imported buffer restores its
on-disk version for dependent requests.

| Request | Answer |
| - | - |
| diagnostics | exactly those of `flick check`, with their `FLICK-` codes, at UTF-16 positions, published when a document is opened or saved; help lines follow the message, related spans (where an expected type came from) are `relatedInformation`, unused code is tagged `Unnecessary` |
| `textDocument/hover` | source signatures and `///` documentation for public and private values, at declarations and uses; module documentation on qualifiers such as `Sql`; inferred locals and values, SQL aliases, columns and parameters, on a keyword that opens an expression (`task`, `scope`, `match`, `fun` …) the type of the whole expression, on `await` and `ignore` the type of the operand (see [keywords.md](../keywords/#in-the-editor)); declarations distinguish `type error` from `service` |
| `textDocument/definition` | across modules; module qualifiers lead to their module header, members to their declaration; built-ins open as real read-only `.fl` files; installed packages use their published `.fl`, else `.iface`; companions lead to their type, class methods at a known instance to that instance |
| `textDocument/references` | every occurrence in the project |
| `textDocument/rename` | the project's own values, types, constructors and locals, in `.fl` sources only; names of built-in modules and packages, service operations and instance methods have no exact mapping and are refused |
| `textDocument/completion` | items carry their `///` documentation and a snippet of a function's parameters, sorted locals first; a value of a module the file does not import is offered as `Module.name` (built-ins need no import, a project module gets its `import` added); record fields after `x.`, module exports or service operations after `M.`, otherwise names in scope; SQL blocks offer tables, aliases, columns, parameters, clauses and functions; `sql table` columns offer the SQL types of the project's dialect, then the modifiers that are not written yet and do not conflict, after `as` Flick types and after `via` the codecs in scope whose type instantiates to the column's `PureCodec`, imported ones also as `Module.codec` |
| `textDocument/codeAction` | quick fixes of diagnostics that carry an edit: the closest name for a misspelt one, removing unused imports and definitions, `_` for an unused variable, the missing arms of a `match`, a `todo` definition of a missing function, the inferred type for a `_` hole; on `FLICK-T004` it inserts the inferred signature `name : T` right above the `let`, at its indentation, with effect rows, type variables, class constraints and names qualified as the module's imports allow; a binding the checker rejected gets none |
| `textDocument/codeAction` (refactors) | add the signature of a `let` value; pattern match on a variable that is a block's result; qualify an imported name at every use, or drop a qualifier and import the name; extract the selected expression into a `let`; inline a local `let` into its uses |
| `textDocument/formatting` | `flick fmt` |
| `textDocument/documentSymbol`, `workspace/symbol` | declarations, with constructors and methods under their type or class; workspace symbols match the letters of the query in order |
| `textDocument/signatureHelp` | in a call `f a b`, the signature of `f` with the argument being written marked |
| `textDocument/typeDefinition` | the declaration of the type of the expression under the cursor |
| `textDocument/semanticTokens/full` | what each name is: module, type, constructor, function, variable, method or parameter |
| `textDocument/foldingRange` | declarations, `match`, records, lists, embedded blocks and the run of imports |
| `textDocument/inlayHint` | the type of each `let` without a signature, and of each stage of a pipeline written over several lines |

Hover resolves source bindings, not just the public export list. Private
helpers such as a view or an error reporter show their written signatures
and documentation without making them public. The `ignore` keyword shows its
discard type at that use (for example `Never -> Unit` around `Ui.Dom.mount`);
discarding a result does not discard the expression's effects or errors.
The [Gleam language server reference](https://gleam.run/documentation/language-server-reference/)
is the behavior reference for symbol hover and navigation, not a claim of
complete feature parity.

Hover on `..` in `MonadError(..)` or `ExceptT(..)` lists the methods or
constructors made visible by that import or export. Each member has its full
Flick signature, including quantifiers, class constraints and captures, plus
its `///` documentation when available. Re-exports list only the members exposed
by the named module; hidden members stay hidden.

Type-alias hover includes generic parameters and the written definition, even
for private aliases. On the declaration name, its hover range includes the
parameters (`Planner a`); navigation and semantic tokens still target `Planner`.
Hover on a label inside `codec` shows the decoded record field's type, such as
`email : Email` or `nickname : Option String`, rather than its field codec.
These signatures use a `flick` code block for editor syntax highlighting.

Hover on `sql table` shows the row type and the type of `Name$table` in
column order; on the name, the row type and the SQL table; on the table
name string, the table it names; on a column's SQL type, the Flick type the
dialect gives it; on a modifier, `as` and `via`, what they mean.

SQL column hover and completion preserve nullability: a column from a
`left_join` reads as `Option`, without adding a second `Option` to an already
nullable column. Nested queries have their own table aliases.

Built-in definitions use ordinary `file://` URIs in both Zed and VS Code.
The server materializes its embedded sources under `flick-lsp` in the system
temporary directory, keyed by content and read-only. Opening them checks
them as library sources, without adding them to the application's project.
Both `textDocument/prepareRename` and `textDocument/rename` reject these
read-only documents rather than returning edits to the cached source.

### Logging

LSP logs use `tracing-subscriber` on stderr, with UTC timestamps and no ANSI
escapes. The default `info` level reports startup, shutdown and failures.
Enable request and analysis details with:

```sh
RUST_LOG=flick::lsp=debug flick lsp --stdio
```

Debug events include the method, request ID, document URI and position,
elapsed time, snapshot reuse, and module, diagnostic and completion counts.
Request failures include their JSON-RPC error code. Panics include the active
request context and a captured backtrace without requiring `RUST_BACKTRACE`.
`RUST_LOG=off` disables ordinary logs; panics still use Rust's default report.

In Zed, set `lsp.flick.binary.env` to
`{ "RUST_LOG": "flick::lsp=debug" }`. Logs never go to protocol stdout.
Normal events omit document bodies and raw RPC payloads, but a panic message
can contain a source excerpt: check logs before sharing them.

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
