---
title: "Plugins"
---

A plugin owns blocks of the modules that import it, expressions such as
`head { … }`, `head "x" (e) { … }` and `head Shape`, and declarations such
as `head table Name "t" { … }` and `head fn ping x = …`, and gives back the
code each block stands for: an expression, or declarations of the module. The compiler checks, compiles and
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

A head is a plugin block only in a file that imports it: elsewhere
`pairs { x = 1 }` is still a call with a record, and every file still parses
on its own. A head is no value, so what follows it on its line is its
arguments, words, names, numbers, strings, paths such as `App.Api.place` and
`( … )`, then maybe a body `{ … }`:

| Form | Where | Example |
| - | - | - |
| `head args { body }` | an expression | `pairs "a" (1.0 + 2.0) { x = 1.0 }`, `sql p { … }` |
| `head args` | an expression | `named Shape`, `server App.Api.place` |
| `head args { body }` | the start of a declaration | `sql table Users "users" { … }` |
| `head args rest` | the start of a declaration | `server fn ping (n : Int) = n + 1` |

The arguments, the body and the rest are kept as tokens, brackets balanced,
for the plugin to read. The rest of a declaration runs to the next
declaration, a line at column 1 that is not a closing bracket. Raw text, such
as SQL with `'quotes'` and `-- comments`, is a raw or multi-line string
argument, `head migration 1 "init" r#"…"#`, which `flick_plugin::string_value`
gives the value of.

The compiler comes with the plugin of `std`, whose heads are `cli` (see
[command-line applications](../cli-dsl/)), `server` (see
[server functions](../server-fn/)) and `codec` (see
[the codec DSL](../schema/#the-codec-dsl)). `cli "x" { … }`, with its body, and
`server fn f x = …` or `server f` are blocks even in a file that does not
import them, so the compiler can say which import they need: `FLICK-P037`,
whose fix `flick check --fix` applies.

The plugin is the one of the package the imported module belongs to; a
module of the project or a built-in one has none. The code a plugin gives
is resolved in the file's scope: its imports, the imported module among
them, the Prelude and the module's own declarations. A plugin should name
what it uses with qualified names, such as `Pairs.make`; another module it
needs is imported by the user. Its names are its own:

- a local of the code, as `x` in `fun x -> …`, is seen only by that code,
  and a local of the user's, around the block or in it, only by the user's
  code: neither captures the other;
- `Pairs.make` names the module `Pairs` by its path, even in a file that
  imports it as `import Pairs as P`, and a module the file renames to
  `Pairs` does not take its place;
- a name the module declares too is still `FLICK-R008` on the block: what
  a block declares is the module's.

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

A block inside the user's code of another, as `subcommand cli "x" { … }`
in a `cli` block, is expanded after it, in a round of its own: `collect`
learns from the blocks of a round together, and a block nested in more than
eight is `FLICK-P036`.

Editors also ask a plugin about its blocks, even ones that do not expand:
`complete(block, state, offset)` gives what to insert at an offset (an
empty list leaves the user's code to the usual completion), `hover(block,
state, offset)` what a span means, and `highlight(block, state)` the spans
of the plugin's own words, as keywords or parameters.

The formatter asks `format(block, state)` for the layout of a block: pieces
of the plugin's words, new lines, a level in or out, and holes, the user's
code, which the formatter lays out as any other. The compiler writes the head
before them, `{` before a layout that starts in the body, and `}` after it,
and keeps the layout only when the declaration parses back to the same code.
Without one, or for a block with comments other than `///` lines, the block
stays as written, its lines moving with its head. `flick fmt` asks the plugins
of the project's packages, an editor those its analysis loaded.

A block shows its head; its `place`, an expression or a declaration; its
text, from its first argument (or from the body, without arguments) to the
closing `}` or its end; `body`, where the body starts in that text (its
length without one); and the tokens of the arguments, the body and the rest,
`{` left out. All spans count from the start of the text. The user's code in
it, the holes, is parsed by the compiler: `parse-expr(start, end)`,
`parse-type`, `parse-pat` and `parse-decl` give the longest expression, type
or pattern from `start` on, or the declaration there, with the layout of
where it stands in the file; `parse-body` gives a body as after the `=` of a
`let`, the lines of a block when it starts a line of its own. A block also
shows `module`, the module it is in, and for a declaration right below a
signature, `signature`: its text and tokens, spans counting from its start.
Code with a type of the signature, `signed(span)`, takes it from the module;
without one, the signature stays where it is, above what the code declares.
`literal-of(name)` says whether the type `name`, as the module writes it
(`Role`, `Roles.Role`), is an enum the module declares or imports, and gives
its base type, `String` or `Int`: the types are not checked yet, so a
plugin asks before it writes code for one.

The code of a block that is an expression is an expression; that of a
declaration is declarations, which take the block's place among the module's
and are the module's like any other: exported with `exposing`, seen by its
importers, a signature above the block for the first of them with its name.
A `let` or a signature may declare a companion `Users.table` of a type
`Users` of the module, exported with the type and named `Users.table` by
importers; the `///` lines above a declaration's block are the block's
`docs`, which the plugin gives to what it declares. Going to the definition
of a generated name goes to the span the plugin gave its declaration, the
user's name in the block or the whole block, and renaming one is renaming
it in its block.
The code is an arena of core syntax, each node after the nodes it refers to
and with the span of the block it stands for:

| Kind | Nodes |
| - | - |
| expression | name, number, text, boolean, unit, application, lambda, record, field, list, tuple, annotation, hole |
| type | name, application, record, function, tuple, type-level text (`"name"`), `_`, `forall`, row, hole, a type of the signature |
| pattern | variable, wildcard, constructor, record, tuple, hole |
| declaration | `let`, signature, `type alias`, `type` with constructors and `deriving`, `instance` with members, a group of several, documentation of one, hole |

The compiler checks the arena before it uses it: names must lex as names,
numbers be finite, holes lie in the body and not overlap, an expression or a
pattern be used once (a type may stand in several places, as in a signature
and an annotation), and its size and depth are bounded. A function in a
plugin's code needs no signature: its type is inferred. The arena version is the version of the interface,
`0.1.0`. Until Flick 0.2 the interface changes without a new version: build
a component with the `flick-plugin` of the compiler it runs in. A component
built for another version does not load: the import that exposes its heads
is `FLICK-P035`, naming the version it was built for and the one the
compiler speaks, until it is rebuilt.

## Checks with types

A plugin's code is checked like the user's, so a mistake there is reported
on what it stands for. What only the types show, such as a field a record
lacks, the plugin says itself after checking: each `_` of its code, a type
the checker infers, is a fact with the span the plugin gave it, and `check`
gets them, the type as written (`shown`), qualified with aliases expanded
(`exact`, none while it has errors or unknowns), the exact base type of an
enum (`literal-of`, `String` or `Int`) and, for a closed record, its fields. `check` only adds errors; a block whose code has no `_` is not
asked. An editor's `hover` gets the same facts, so a plugin can show the type
of what its block names, as `codec` shows a field's over its label.

```rust
fn check(&self, block: &Block, _: &[u8], facts: &[Fact]) -> Vec<Diagnostic> {
    facts.iter().map(|f| Diagnostic::error(format!("`_` is `{}`", f.ty.shown), f.span)).collect()
}
```

## Derives

A class marked `@derive` is derived by a plugin: that of the class's
package, or of the package the attribute names, `@derive std`.

```flick
@derive
class Arbitrary a =
  arbitrary : a
```

A type that names such a class in `deriving`, maybe with arguments
`deriving Arbitrary { depth = 3 }`, and a `deriving instance Arbitrary
(Shapes.Shape)` for a type of another module of the package, call
`derive(target, class, args, module, state)` of the plugin:

- `target` is the type's declaration, a block, and what it declares at spans
  of it: its name, parameters, constructors with the types of their fields
  and, of an enum, their literals, whether a constructor of a GADT names
  the type it builds (`refined`) or holds dictionaries (`constrained`), the
  type an alias stands for and the base of a refined type;
- `class` is the class's full name, `Pairs.Arbitrary`;
- `args` the `{ … }` after the class;
- `module` every type the module declares.

The plugin gives declarations, an instance and what goes with it, which
follow the type's declaration, or take the place of `deriving instance`.
Their errors are on the class in `deriving`. A type of another module is
named qualified, `Shapes.Square`, and its spans are no holes; the instance
is still the module's, under the rule of orphans (`FLICK-C006`).

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
    fn collect(&self, _: &[Block], _: &mut dyn Compiler) -> Vec<u8> {
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
`flick!(b, type T)` builds a type. Fixed names are written as they are:
`flick!(b, Cli.flag (Record.label : Record.Label #field) [#(..&aliases)] #help #multi #spec)`.
Code that starts as a declaration builds one, its name written or
`#(expr)`: `flick!(b, let #(name) x : Float = f x)`, `flick!(b, size : Int)`,
`flick!(b, type alias #(name) = { #(..fields) })`,
`flick!(b, type Color = Red | Rgb Float Float deriving Show)` and
`flick!(b, instance Show #(Name(t)) { let show _ = #t })`; `b.group(ids)`
gives several, `b.doc(&block.docs, id)` documents one, `let #("Users.table")`
declares a companion, and a `Span` in a declaration's place is the user's.
`flick!(b @ name, let …)` declares it at the user's `name`, where editors go.

A block is read with a cursor over its tokens, `block.cursor()` from the
first or `block.body()` from the body: `eat(":")`, `ident()`, `strings()`,
`rest()`, and `expr(compiler)`, `ty`, `pat` or `decl`, which ask the
compiler for the user's code from the next token on and go past it.
`block.arguments()` are the tokens of the head's arguments, and
`block.docs_above(offset, " ")` the `///` lines above what starts at
`offset`. `Plugin::format` gives `Piece::Text`, `Line`, `Indent`, `Dedent`
and `Hole(hole)`, a hole as the cursor's `expr` and `ty` find them.

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

## Reaching outside

A plugin reaches nothing but its blocks unless the project that uses it
grants more in its `flick.toml`; the package of the plugin declares nothing:

```toml
[plugins.sql]
fs-read = ["schema/", "db/extra.sql"]   # from the project's root: a directory and what it holds, or a file
net = ["db.example.com", "localhost:8080"]  # a host at any port, or at that one
env = ["SQL_TOKEN"]
```

`collect`, `expand` and `derive` ask through the compiler:
`compiler.read_file("schema/users.sql")`, `compiler.env("SQL_TOKEN")` and
`compiler.fetch(&Request { method, url, headers, body })`, which give the
bytes, the value (`None` when it is unset) and the `Response { status, body }`.
What the project does not grant is `ReachError::Denied`, and
`FLICK-P038` on the block, naming the line to add, whatever the plugin makes
of the error; a path with `..` or a root is never granted, nor is a URL
other than `http` or `https`, one with `user@`, or one whose host is in
brackets. A database is reached the same way, over HTTP or through a schema
the project commits; the compiler has no drivers. Blocks of a library's
modules reach nothing, and neither do `complete`, `hover`, `highlight` and
`format`.

An answer over HTTP is kept as a snapshot, the JSON file
`.flick/plugin-snapshots/<package>/<hash>.json` in the project, which is
committed: the hash is over the method, the URL and the body. Headers are
not part of it, so a token goes in a header, never in the URL; a changed
header changes no snapshot. A run asks
the network only for a request no snapshot answers, follows no redirect,
waits at most 30 seconds and takes at most 16 MiB; an answer with a status
of 500 or more is not kept. With `--offline`, or `CI` set, and in the
editor, a request no snapshot answers is `ReachError::Offline` and
`FLICK-P039`. Deleting a snapshot fetches the answer again. Files and
variables are read as they are on each run.

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
A kept result also records the hash of each file, variable and snapshot its
call reached, and holds only while each is the same; the build cache counts
them too. `FLICK_PLUGIN_TRACE=<file>` appends a line for each call a plugin
answers.

## Diagnostics

| Problem | Diagnostic |
| - | - |
| the plugin a module imports cannot be found or loaded | `FLICK-P035` on the import |
| a plugin refuses a block, fails or gives code that does not fit | `FLICK-P036` on the block, or at the span the plugin names |
| an error the plugin gives under its own code, such as `L001` | `<package>/L001`, as `std/L001`, at the span it names |
| a head of the compiler's own plugins whose import the file lacks | `FLICK-P037` on the head, with the import as its fix |
| an error in the user's code inside a block | the usual one, at that code |
| an error in the code a plugin gave | the usual one, on the code it stands for, with a note naming the plugin |
| what a plugin's `check` finds | `<package>/X001` at the span it names; `FLICK-P036` when `check` fails |
| a plugin reaches a file, a variable or a host the project does not grant | `FLICK-P038` on the block |
| an offline run has no snapshot of the answer a plugin asks for | `FLICK-P039` on the block |

A span of the whole text of a block is the whole block, its head with it.
A plugin's code is capital letters and three digits; the compiler puts the
package before it, so a plugin cannot speak for another.
`flick explain std/L001` shows `explain/L001.md` of the package. Warnings
about generated code are not reported, and fixes that would edit it are
dropped.

## Not yet

Warnings of plugins come later. A
built-in module the file hides by an alias, as `import Other as List`, is
hidden from generated code too. `collect` sees the modules the run loads, so `flick check one.fl` gives it
fewer blocks than a whole build. A package whose modules have plugin blocks
is checked again in each build.
