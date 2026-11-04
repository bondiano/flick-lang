---
title: "Plugins"
---

A plugin owns blocks `head { … }` and `head "x" (e) { … }` of the modules
that import it and gives
back the code each block stands for. The compiler checks, compiles and
shows that code in place of the block, while the user's code inside the
block keeps its own place in the file: errors, hovers and renames land on
it.

## Using a plugin

A library provides the plugin; the project depends on the library as on
any other (see [project.md](../project/)). A module imports a module of the
library and exposes the plugin's heads with `plugin`, beside the module's
own names:

```flick
module App.Main exposing (point)

import Pairs exposing (Pair, plugin pairs)

point : { x : Float; label : String; blocks : Float }
let point = pairs { x = 1.0 + 2.0; label : String = "a" }
```

A head is a plugin block only in a file that imports it, and only before
`{`: elsewhere `pairs { x = 1 }` is still a call with a record, and every
file still parses on its own. Between the head and `{`, on the head's line,
come its arguments: strings and `( … )`, as in `pairs "a" (1.0 + 2.0) { … }`.
The arguments and the body are kept as tokens, brackets balanced, for the
plugin to read.

The compiler comes with the plugin of `std`, whose head is `cli` (see
[command-line applications](../cli-dsl/)). Its head is a block even in a file
that does not import it, so the compiler can say which import it needs:
`FLICK-P037`, whose fix `flick check --fix` applies.

The plugin is the one of the package the imported module belongs to; a
module of the project or a built-in one has none. The code a plugin gives
is resolved in the file's scope: its imports, the imported module among
them, and the Prelude. A plugin should name what it uses with qualified
names, such as `Pairs.make`; another module it needs is imported by the
user.

## What a plugin gives

A plugin is a WebAssembly component of the world `flick:plugin` in
[`flick-plugin/wit/plugin.wit`](https://github.com/bondiano/flick-lang/blob/main/flick-plugin/wit/plugin.wit),
or a Rust value of the `Plugin` trait of the `flick-plugin` crate linked
into the compiler. It has two calls, both pure:

- `collect(blocks)` learns from all its blocks of a group (the project's
  sources, its tests, or a package), in the order of their modules' names
  and places, and gives a state of bytes;
- `expand(block, state)` gives the code of one block, or what it finds
  wrong at spans of the block.

Editors also ask a plugin about its blocks, even ones that do not expand:
`complete(block, state, offset)` gives what to insert at an offset (an
empty list leaves the user's code to the usual completion), `hover(block,
state, offset)` what a span means, and `highlight(block, state)` the spans
of the plugin's own words, as keywords or parameters.

A block shows its head; its text, from its first argument (or from the body,
without arguments) to the closing `}`; `body`, where the body starts in that
text; and the tokens of the arguments and the body, `{` left out. All spans
count from the start of the text. The user's expressions and types in it,
the holes, are parsed by the compiler: `parse-expr(start, end)` and
`parse-type(start, end)` give the longest expression or type from `start`
on, with the layout of where it stands in the file.

The code is an arena of core syntax, each node after the nodes it refers
to and with the span of the body it stands for:

| Kind | Nodes |
| - | - |
| expression | name, number, text, boolean, unit, application, lambda, record, field, list, tuple, annotation, hole |
| type | name, application, record, function, type-level text (`"name"`), `_`, hole |
| pattern | variable, wildcard, constructor, record |

The compiler checks the arena before it uses it: names must lex as names,
numbers be finite, holes lie in the body and be used once, and its size and
depth are bounded. The arena version is the version of the interface,
`0.2.0`; a component built for another version does not load.

## Writing one in Rust

`flick new demo --template plugin` makes a library with a plugin: the module
programs import (`src/Tagged.fl`), the crate of the plugin (`plugin/`), the
explanations of its codes (`explain/`) and the script `flick run plugin`,
which builds the component `plugin.wasm` the manifest names with
`[package] plugin`. Its plugin is `tagged "name" { value }`, which stands for
`{ tag = "name"; value = value }`:

```rust
use flick_plugin::{Arena, Block, Builder, Compiler, Diagnostic, Plugin, TokenKind, flick};

pub struct Tagged;

#[cfg(feature = "guest")]
flick_plugin::export!(Tagged);

impl Plugin for Tagged {
    fn collect(&self, _: &[Block]) -> Vec<u8> {
        Vec::new()
    }

    fn expand(
        &self,
        block: &Block,
        _: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        let tag = match block.arguments() {
            [t] if t.kind == TokenKind::String => t.span,
            _ => {
                let message = "`tagged` takes its tag: `tagged \"name\" { value }`";
                return Err(vec![Diagnostic::error(message, block.whole()).with_code("T001")]);
            }
        };
        let mut body = block.body();
        let value = body.expr(compiler).map_err(|d| vec![d])?;
        let mut b = Builder::new(block.whole());
        let record = flick!(b, { tag = #tag; value = #value });
        Ok(b.finish(record))
    }
}
```

`flick!(b, code)` builds the code it is given, written in Flick, when the
plugin is compiled; what is not Flick the arena holds is an error of
`cargo build`. `#x` and `#(expr)` put a value of the plugin in place, by
where it stands:

| Value | In an expression | In a type |
| - | - | - |
| `Span` | the user's code there, a hole | the user's type there |
| `&str`, `String` | a text | a type-level text, as in `Record.Label "x"` |
| `Name("M.f")` | a name | a name |
| `Wildcard` | — | `_` |
| a `NodeId` built before | that node | that node |

`#(..xs)` puts each item of a collection in a list, in the arguments of an
application or in the fields of a record (`(label, value)` pairs);
`flick!(b @ span, code)` gives the nodes their origin, and
`flick!(b, ty T)` builds a type. Fixed names are written as they are:
`flick!(b, Cli.flag (Record.label : Record.Label #field) [#(..&aliases)] #help #multi #spec)`.

A block is read with a cursor over the tokens of its body: `eat(":")`,
`ident()`, `strings()`, and `expr(compiler)` or `ty(compiler)`, which ask
the compiler for the user's code from the next token on and go past it.
`block.arguments()` are the tokens of the head's arguments, and
`block.docs_above(offset, " ")` the `///` lines above what starts at
`offset`.

The template's tests show what the plugin makes of a block, through
`flick_core::plugin::testing::expand`:

```rust
let found = expand(&Tagged, "module M exposing (x)\n\nimport Tagged exposing (plugin tagged)\n\nlet x = tagged \"a\" { 1.0 + 2.0 }\n");
assert_eq!(found, [Ok("{ tag = \"a\"; value = (1.0 + 2.0) }".to_owned())]);
```

In a program, `flick expand` prints the code each block of the project, or
of the files given, stands for, the user's code in place. Built with
`crate-type = ["cdylib"]`, the `guest` feature of `flick-plugin` and
`--target wasm32-wasip2`, as the script does, the crate is a component.

## Limits and caches

Each call runs in a fresh instance with a budget of fuel and 64 MiB of
memory, and every import but the compiler's traps: a plugin that spins or
takes all memory is stopped. Release builds of `flick` run components; a
build without the `wasm-plugins` feature says so.

Since calls are pure, their results are kept: in memory for an editor
session, and in the user's cache directory, keyed by the plugin (its
package and the hash of its component), the block's text and where its
body starts, and the state collected. A second build calls no plugin; a
changed block costs one `collect` and one `expand`; a new block changes
what `collect` learns, and every block of its group is expanded again.
`FLICK_PLUGIN_TRACE=<file>` appends a line for each call a plugin answers.

## Diagnostics

| Problem | Diagnostic |
| - | - |
| the plugin a module imports cannot be found or loaded | `FLICK-P035` on the import |
| a plugin refuses a block, fails or gives code that does not fit | `FLICK-P036` on the block, or at the span the plugin names |
| an error the plugin gives under its own code, such as `L001` | `<package>/L001`, as `std/L001`, at the span it names |
| a head of the compiler's own plugins whose import the file lacks | `FLICK-P037` on the head, with the import as its fix |
| an error in the user's code inside a block | the usual one, at that code |
| an error in the code a plugin gave | the usual one, on the code it stands for, with a note naming the plugin |

A span of the whole text of a block is the whole block, its head with it.
A plugin's code is capital letters and three digits; the compiler puts the
package before it, so a plugin cannot speak for another.
`flick explain std/L001` shows `explain/L001.md` of the package. Warnings
about generated code are not reported, and fixes that would edit it are
dropped.

## Not yet

Declarations and modifiers, raw-text blocks, permissions, warnings of
plugins and checks after typing come later. A plugin block inside the
user's code of another block is not a block. `collect`
sees the modules the run loads, so `flick check one.fl` gives it fewer
blocks than a whole build. Generated names are not hygienic. A package whose
modules have plugin blocks is checked again in each build.
