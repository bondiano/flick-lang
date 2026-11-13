---
title: "Your own typed DSL"
---

A DSL should turn domain language into an ordinary checked value, not a string
that bypasses the compiler. This guide builds `validate { positive 42 }`:
its result is `Result ValidationError Int`. The complete
[working example](https://github.com/bondiano/flick-lang/tree/main/examples/typed-dsl)
contains the Rust plugin, a Flick library and its consuming app.

## Start with the result

```flick
module PositiveDsl exposing (ValidationError(..), positive)
type ValidationError = NotPositive deriving Show
positive : Int -> Result ValidationError Int
let positive n = if n > 0 then Ok n else Err NotPositive
```

`Err NotPositive` is a domain result, not a compiler failure. Zero is a valid
Int input; the program decides what to do with its refusal. A String is not an
Int input and must be rejected by the compiler instead.

## Build and import the plugin

From the repository root:

```sh
pnpm install --frozen-lockfile
pnpm -C runtime build
pnpm -C packages/effect-ts build
cargo build -p flick-example-typed-dsl --features guest --target wasm32-wasip2 --release
cargo build -p flick --features wasm-plugins
node examples/typed-dsl/check.mjs
```

The check script copies the component into the library, checks accepted and
rejected programs, and executes both domain outcomes. It restores the app's
source even if a check fails. The compiler must support WASM plugins.

The library's `flick.toml` names `plugin = "validation.wasm"` under `[package]`;
the app depends on `validation = { path = "../library" }`. Its module imports
both the result type and the plugin head:

```flick
module Main exposing (answer)
import PositiveDsl exposing (ValidationError, plugin validate)
answer : Result ValidationError Int
let answer = validate { positive 42 }
```

Markdown doctests trust imported packages. The example's project check above
is what proves this external plugin really expands and runs.

## Read syntax, return code

Read `plugin/src/lib.rs` alongside this sequence:

1. `collect` returns empty state: this DSL has no cross-block schema.
2. `expand` finds the first body token. Anything other than `positive` reports
   `validation/V001` at that token's span.
3. `Compiler::parse_expr` parses the expression after that word. It is a hole,
   not text re-parsed by the plugin; trailing tokens are rejected.
4. `flick!(b, PositiveDsl.positive #hole)` builds an arena. Generated names are
   qualified; the user's expression keeps its own origin.
5. Flick resolves and checks that arena. `positive "42"` therefore fails with
   `FLICK-T003`, while `positive 0` checks and returns `Err NotPositive` at runtime.

The plugin runs as a WASM component of the versioned SDK, not a shell command.
This example needs no filesystem, network or environment permissions. See
[Plugins](../../reference/plugins/) for the protocol, resource limits and
permissions when your DSL needs external inputs.

## Editor hints belong to the same syntax

`complete` offers `positive` with its result signature; a member-completion
request is left to the compiler. `hover` identifies the keyword span and
explains the domain refusal. Spans are UTF-8 byte ranges relative to the block,
not absolute file offsets. The host maps them back into the user's file.
These hooks do not replace type checking, and this minimal example does not
implement keyword highlighting or a custom formatter.

## SQL from migrations: an LLM repair loop

A larger DSL can publish a schema and check other blocks against it. Flick's
SQL plugin derives `People` from the migration below, without connecting to a
server. Suppose an LLM produces this query:

```flick fail std/Q001
# import Sql
sql migration 1 "people" {
  create table people (id bigint primary key, name text not null);
}
let names = sql { from p in People select { name = p.nickname } }
```

Save it as a Flick module and run `flick check`. The unknown column is
`std/Q001`: `nickname` is not in the migration-derived table. Give the diagnostic
and schema back to the generator; the corrected block is:

```flick
# import Sql
sql migration 1 "people" {
  create table people (id bigint primary key, name text not null);
}
names : Sql.Template Sql.Pg {} { name : String }
let names = sql { from p in People select { name = p.name } }
```

Run `flick check` again. The explicit signature now checks both the PostgreSQL
template and its row. This loop is deterministic and needs no LLM service in CI.
Use compiler diagnostics as feedback, not a claim that generated code is correct.

## What is and is not proved

- The compiler checks generated expressions, input types and declared result
  types. The plugin owns its grammar and domain-specific static rules.
- SQL checks against the source schema and its supported SQL subset. It does not
  prove that a running database has applied those migrations, that credentials
  work, that a query is fast, or that the selected data matches business intent.
- A domain `Result` remains something the caller must handle. Neither compiling
  nor a successful example proves that an arbitrary plugin implements its intended
  specification. Test rejected cases and runtime results independently.

## Nearby approaches

| Approach | Syntax and type information | Diagnostics and tools |
| - | - | - |
| [Kotlin DSL](https://kotlinlang.org/docs/type-safe-builders.html) | Ordinary Kotlin builders and receiver lambdas; types come from library APIs. | Ordinary compiler/IDE support; `@DslMarker` restricts receiver scopes. |
| [Rust proc macros](https://doc.rust-lang.org/reference/procedural-macros.html) | Token streams expand into Rust syntax, which is then checked. | Macro authors control spans and emitted errors; expansions are unhygienic. |
| [F# type providers](https://learn.microsoft.com/en-us/dotnet/fsharp/tutorials/type-providers/) | Providers expose types and members based on external data or schema inputs. | Compiler tooling sees provided types; guarantees depend on provider inputs and implementation. |
| Flick plugins | Imported heads own block syntax and return core syntax arenas with typed holes. | Explicit diagnostic, completion and hover hooks share block spans; generated code is checked normally. |

Choose ordinary library functions when they express the domain clearly. Use a
plugin when domain syntax or schema-aware feedback pays for maintaining a parser,
editor hooks and regression tests. Continue with [SQL](../../reference/sql/) and
[testing](../../contributing/testing/).
