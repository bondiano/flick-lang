---
title: "Plugins"
---

A plugin owns blocks `head { … }` of the modules that import it and gives
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

A head is a plugin block only in a file that imports it, and only right
before `{`: elsewhere `pairs { x = 1 }` is still a call with a record, and
every file still parses on its own. The body of a block is kept as tokens,
braces balanced, for the plugin to read.

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
  wrong at spans of the body.

A block shows its head, the text of its body and the body's tokens, all
spans counted from the start of the body. The user's expressions and types
in it, the holes, are parsed by the compiler: `parse-expr(start, end)` and
`parse-type(start, end)` give the longest expression or type of the body
from `start` on, with the layout of where it stands in the file.

The code is an arena of core syntax, each node after the nodes it refers
to and with the span of the body it stands for:

| Kind | Nodes |
| - | - |
| expression | name, number, text, boolean, unit, application, lambda, record, field, list, tuple, annotation, hole |
| type | name, application, record, function, hole |
| pattern | variable, wildcard, constructor, record |

The compiler checks the arena before it uses it: names must lex as names,
numbers be finite, holes lie in the body and be used once, and its size and
depth are bounded. The arena version is the version of the interface,
`0.1.0`.

## Writing one in Rust

```rust
use flick_plugin::{Arena, Block, Builder, Compiler, Diagnostic, Expr, Plugin, Span};

pub struct Echo;

impl Plugin for Echo {
    fn collect(&self, _: &[Block]) -> Vec<u8> {
        Vec::new()
    }

    fn expand(
        &self,
        block: &Block,
        _: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        let end = u32::try_from(block.text.len()).unwrap_or(u32::MAX);
        let hole = compiler.parse_expr(0, end).map_err(|d| vec![d])?;
        let mut b = Builder::new(Span { start: 0, end });
        let root = b.at(hole.span).expr(Expr::Hole(hole.span));
        Ok(b.finish(root))
    }
}

#[cfg(feature = "guest")]
flick_plugin::export!(Echo);
```

Built with `crate-type = ["cdylib"]`, the `guest` feature of `flick-plugin`
and `--target wasm32-wasip2`, the crate is a component; the library names it
with `[package] plugin = "echo.wasm"`, a path inside the library.

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
| an error in the user's code inside a block | the usual one, at that code |
| an error in the code a plugin gave | the usual one, on the code it stands for, with a note naming the plugin |

Warnings about generated code are not reported, and fixes that would edit
it are dropped.

## Not yet

The forms with arguments, declarations and modifiers, raw-text blocks,
permissions, codes of plugins and checks after typing come later. `collect`
sees the modules the run loads, so `flick check one.fl` gives it fewer
blocks than a whole build. Generated names are not hygienic. A package whose
modules have plugin blocks is checked again in each build.
