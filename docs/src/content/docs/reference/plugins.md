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
[the codec DSL](../schema/#the-codec-dsl)), and `sql`. Every module has these
heads without an import: `cli "x" { … }`, with its body, and `server fn f x =
…` or `server f` are blocks of `std`'s plugin, and the words stay ordinary
names elsewhere. Only a block without `{`, such as `cli Main`, needs `import
Cli exposing (plugin cli)`; elsewhere such an import is `FLICK-R012`, which
`flick check --fix` removes. Another plugin may not give these heads:
`FLICK-P044`.

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
into the compiler. It has three calls, all pure:

- `collect(blocks)` learns from all its blocks of a group (the project's
  sources, its tests, or a package), in the order of their modules' names
  and places, and gives a state of bytes;
- `publish(state)` gives what the blocks of other modules and groups may
  ask of a group's state: names of modules, each with bytes, as a table's
  columns. Every group publishes before any block expands, so a module of
  the tests sees what the sources published;
- `expand(block, state)` gives the code of one block, or what it finds
  wrong at spans of the block.

A block inside the user's code of another, as `subcommand cli "x" { … }`
in a `cli` block, is expanded after it, in a round of its own: `collect`
learns from the blocks of a round together, and a block nested in more than
eight is `FLICK-P036`.

Editors also ask a plugin about its blocks, even ones that do not expand:
`complete(block, state, offset, member)` gives what to insert at an offset
(an empty list leaves the user's code to the usual completion; `member`
says the user typed `name.` there, the `.` not in the block, and the
compiler answers `published` as while the block expands; there
`in-scope(ns, fits)` gives the values or types in scope of the block's
module, by the names it writes them with, and with `fits`, a type written as
the module writes it or with names qualified in full (`Prelude.PureCodec
Prelude.BigInt UserId`), only the values whose type instantiates to it;
while a block expands it gives none), `hover(block,
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
`let`, the lines of a block when it starts a line of its own; `parse-lines`
gives the same body with the lines under a call as its arguments, one
whole expression each, and the arena stands for its hole with `lines`. A block also
shows `module`, the module it is in, and for a declaration right below a
signature, `signature`: its text and tokens, spans counting from its start.
Code with a type of the signature, `signed(span)`, takes it from the module;
without one, the signature stays where it is, above what the code declares.
An expression that is the whole body of a `let` of the module sees the
signature of that `let` too, but its code takes no type of it.
`literal-of(name)` says whether the type `name`, as the module writes it
(`Role`, `Roles.Role`), is an enum the module declares or imports, and gives
its base type, `String` or `Int`: the types are not checked yet, so a
plugin asks before it writes code for one. `published(name)` gives what the
same plugin published of `name` as the module writes it (`Users`,
`Schema.Users`): a name of the module itself or of one it imports, and
`published-names()` every name it answers. A kept expansion that asked holds
only while the answer stays the same.

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
| expression | name, number, text, boolean, unit, application, lambda, record, field, list, tuple, annotation, hole, concatenation (`a ++ b`), `match` with guards, `if`, a block of statements and its value, a body with a module open |
| type | name, application, record, function, tuple, type-level text (`"name"`), `_`, `forall`, row, hole, a type of the signature |
| pattern | variable, wildcard, constructor, record, tuple, hole |
| declaration | `let`, signature, `type alias`, `type` with constructors and `deriving`, maybe `unboxed`, `instance` with members, a group of several or none, documentation of one, a function with the value that reads a constant given to it, hole |

The compiler checks the arena before it uses it: names must lex as names,
numbers be finite, holes lie in the body and not overlap, an expression or a
declaration be used once (a type may stand in several places, as in a
signature and an annotation, and a pattern too, as the props of a component
its setup and its render bind), and its size and depth are bounded. A block
is only the body of a `fun` or a `let`; its statements are `let`s,
signatures, holes of them or expressions, and a hole of a declaration there is
a `let` or a signature of the user's. A body with a module open names the
module by its path: in the body, a name the module exports is that one,
before the names of the file, unless a local hides it; no source writes
this form, which is how a block brings the names of its library into
sight. With the type of a child, a call in that body gives a last parameter
that is a list of the type element by element: each argument from that
parameter on is one, none is the empty list, one argument that is a list is
the list itself, and a list literal is refused (`FLICK-T073`). With a
function to apply values through, a value in that body whose type is that of
the function's first parameter is applied through it: `badge props` is
`Ui.component badge props` inside `ui`. A whole number is
an integer literal, of any numeric type its use asks for; a declaration
block whose group is empty declares nothing. A function in a
plugin's code needs no signature: its type is inferred. The arena version is the version of the interface,
`0.2.0`. Build
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

## Checks of calls

A package may watch calls of names, not only its own blocks. `watch` of its
`[package]`, beside `plugin`, names them, each `Module.name`,
`Module.Sub.name` or `*.name` for the name in any module. It may also name a
service, `Module.Service`, and a tag, `@maker` or `@cell`; what the plugin
is told of those is below.

```toml
[package]
name = "ui"
version = "1.0.0"
plugin = "ui.wasm"
watch = ["Ui.Event.queue", "*.parse"]
```

`check-calls` then gets every place of a checked module that applies one of
them, with what the checker found there: the module and name applied, the
span of the application, whether it stands in code a plugin wrote, and for
each argument where it is written, its type as `check` sees it, the constant
it folds to, if any, and whether it is written as a `fun`. A call that
gives a constant also carries how the constant is read: for the `parse` of
an enum, its literals; for a function the code of a block declared with the
value that reads its constant (`Decl::Checked { by, decl }`), the syntax and
resolved references of that value, as the check of a refined type reads the
argument of its `parse`. The plugin interprets those facts. Every span is of the
module's own file, inside a block as well as outside one, so a diagnostic
comes back at the code the user wrote.

Each place that names a watched value, applied or not, is a place too, with
read-only `binding` facts and no arguments: the module and the name of the
top-level value it stands in (none for a function), the span of that
declaration, whether its scheme takes a dictionary, whether the reference is
inside a function, and the count of arguments it is applied to. An applied
name is so two places: the application, with its arguments, and the name,
with its `binding`. The UI plugin reads these facts of the serializable
atoms for `FLICK-U003`; they do not change type inference.

A watched service that the signature of a top-level function names is a place
too: at the function's name, with no arguments and with `binding` saying
which function it is and, in `gives`, the type it gives when all its arrows
are applied. The UI plugin warns (`FLICK-U007`) where a function reads atoms
to give a `Ui`.

A handler made of a `fun` inside the render of a `Ui.view` is found the same
way: the calls of `Ui.view` and of the constructors of `Ui.Event` are watched,
and the plugin warns (`FLICK-U002`) where a constructor given a `fun` stands
inside the first argument of a view the module itself writes.

A plugin may also watch a tag, the attribute that marks values of a kind, as
`@maker` marks what makes an atom, an action or a view. Each place inside a
function that names a value the tag reaches, itself or through the functions
it calls, is then a place with `tagged`: the tag, the name as written, the
marked value it ends at, and in `inside` the functions around the place from
the nearest outwards, each with the services of its arrow and whether its
row is open. The task of a handler runs later, so the functions around a
`task` are not around what is inside it. A function that needs a service
marked `@owner` reaches nothing to those that name it: what is made under
such a service belongs to its handler, as what a component's setup makes
belongs to the instance (`Setup` is marked so). The UI plugin refuses
(`FLICK-U006`) a maker named in a render, a function whose services are
`Atom.Read` alone.

A watched service is also a place where the function expected names it and
the code given there asks for more: a service or an error its row does not
allow. The place carries `lacking`: the function expected, what is not
allowed as the compiler words it, the services as shown and by qualified
name, and the errors. The compiler refuses such code itself (`FLICK-E002`);
an error the plugin gives at the span of the place stands for the
compiler's, so the plugin can say where the code belongs. The UI plugin
does so for a render that does more than read atoms (`FLICK-U004`) and for
the setup of a component.

The tag `@cell` marks a type whose values hold one value each. Where a
value that is no function would hold such a type at every type its
variables stand for, the compiler refuses it (`FLICK-T072`) and the place is
one with `tagged`: the tag, in `named` the value's type as shown, in
`reaches` the marked type, and nothing in `inside`; `binding` names the
value, if it has a name. An error the plugin gives there stands for the
compiler's, as above: the UI plugin says it of an atom (`FLICK-U005`).

```rust
fn check_calls(&self, calls: &[CallSite], state: &[u8]) -> Vec<Diagnostic> {
    (calls.iter())
        .filter(|c| !matches!(c.arguments.first().and_then(|a| a.constant.as_ref()),
            Some(Constant::Number(n)) if *n > 0.0))
        .map(|c| Diagnostic::error("it counts from one", c.span).with_code("L002"))
        .collect()
}
```

The plugin is asked once per module, with the calls of the names its
package watches and the state its `collect` made; a module with no such
call does not reach it, and a package without `watch` is asked nothing.
`check-calls` only adds errors: the call itself the compiler has checked
already. The checks of `std` work so, and keep the compiler's own codes: a
policy bound that is not a positive integer is `FLICK-U001`, and a constant
that fails the check of its refined type is `FLICK-D006`.

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
  type an alias stands for and, for a type over another whose constructor
  is its declaration's alone, the type it wraps (`wraps`): the base of a
  refined type, or the field of an `unboxed type` of a block whose
  constructor only generated code can name. The values of such a type come
  from its `T.parse` and are read by its `T.value`;
- `class` is the class's full name, `Pairs.Arbitrary`;
- `args` the `{ … }` after the class;
- `module` every type the module declares.

The plugin gives declarations, an instance and what goes with it, which
follow the type's declaration, or take the place of `deriving instance`.
Their errors are on the class in `deriving`. A type of another module is
named qualified, `Shapes.Square`, and its spans are no holes; the instance
is still the module's, under the rule of orphans (`FLICK-C006`).

A type the code of a block declares names its classes in the `deriving` of
that declaration, and each class's derive is called for it as for a
declaration of the module: the block is its `target`, and the fields of the
type are the types the user wrote in the block. A field the plugin made
itself has no span to give, and a class a plugin derives is then
`FLICK-P036`. The derived code goes with the block's own.

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
`flick!(b, unboxed type #name = #(make) Float)` declares a type of its own
whose values are its field's when the program runs. A constructor named
`Meters$Make` is one only generated code can write, and only the code of
the plugin that declared it may name it: another plugin's code that does is
`FLICK-P036`, and the type leaves its module opaque. The block's own
functions are then the only ways into the type and out of it.

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

## Files of other languages

A plugin may also turn a file of another language into a module. Its package
says which extensions, beside `plugin`:

```toml
[package]
name = "protos"
version = "0.1.0"
plugin = "protos.wasm"
load = ["proto"]
```

Then `import App.User` finds `App/User.proto` when there is no `App/User.fl`,
and the plugin's `load(path, bytes)` gives the module's declarations, each at
the span of the file it stands for: the project checks and imports them as
its own, the module exposes all of them, and the editor goes from a use to
that line of the file, and sends the server the file's edits once the
server registers the extension. What the plugin finds wrong is at the file's
spans under its own codes. The code names std and the Prelude by their full
names; it cannot import another module of the project yet, the file must be
UTF-8 text, and a `.fl` file of the same module beside it is `FLICK-R002`.

## Rewriting the IR

A plugin whose package says `hooks = ["ir"]` gets the IR of the project's
modules, a `Program` of the compiler's IR as CBOR without the built-in
modules, after the checks of effects pass it and before the build optimizes
it, in `flick build`, `run` and `test`, never in `flick check` or the editor.
Its `transform` gives the program the build compiles instead. The compiler
checks it for form only: the version of the IR, every module of the build
and no other, every local bound where it is used, every global of a module
there is, every span of a file the build read; anything else is
`FLICK-P047`, naming the plugin. A build it rewrote says so with the note
`FLICK-P048`: what the program does is then the plugin's to answer for,
outside Flick's checks and its model of the language, and the build is not
cached. Each version of the IR is a breaking change for such a plugin until
the IR is stable.

## Rewriting what a build writes

A plugin whose package says `hooks = ["emit"]` beside `plugin` gets the files
`flick build` writes before it writes them: each path in the output
directory and its text, the `.js` of each module with its `.map` when the
build writes maps, and the `.iface` that sums up its types. Its `emit` gives
the files the build writes instead; the plugins of several packages run one
after another in the order of `[dependencies]`. A plugin that changes a `.js`
it got with a `.map` gives that `.map` changed too, or the build keeps the
files as they were and reports `FLICK-P046`; a `.js` without a map may
change alone. Only `flick build` asks: `flick run` and `flick test` build
without it, so a plugin that changes behavior tests it itself.

## Disabling a plugin

A project may refuse part of a plugin it gets, the compiler's own included, in
the same table: `disable` names heads of blocks and classes the plugin derives.

```toml
[plugins.std]
disable = ["sql", "Cli.Value"]
```

A block of a disabled head and a `deriving` of a disabled class are `FLICK-P042`,
at the block's head or the class, with the manifest's line beside it, in
`flick check` and in the editor alike.

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
| an import of a head of `std`'s plugin from another package | `FLICK-P044` on the import |
| an error in the user's code inside a block | the usual one, at that code |
| an error in the code a plugin gave | the usual one, on the code it stands for, with a note naming the plugin |
| what a plugin's `check` finds | `<package>/X001` at the span it names; `FLICK-P036` when `check` fails |
| what a plugin's `check-calls` finds | `<package>/X001` at the span it names; `FLICK-P036` when it fails, at the first call it was asked about |
| a plugin reaches a file, a variable or a host the project does not grant | `FLICK-P038` on the block |
| an offline run has no snapshot of the answer a plugin asks for | `FLICK-P039` on the block |

A span of the whole text of a block is the whole block, its head with it.
A plugin's code is capital letters and three digits; the compiler puts the
package before it, so a plugin cannot speak for another.
`flick explain std/L001` shows `explain/L001.md` of the package. A
diagnostic with a code may be a warning or a note,
`Diagnostic::warning(message, span).with_code("L010")`, and carry fixes,
`.with_fix("rename it", edits, safe)`, whose edits replace spans of the block;
`flick check --fix` applies the safe ones. One without a code is an error, and
`FLICK-P036`. Warnings about generated code are not reported, and fixes that
would edit it are dropped.

## Not yet

A plugin warns only from `check` or beside a refusal: `expand` gives code or
errors, not code and warnings. A
built-in module the file hides by an alias, as `import Other as List`, is
hidden from generated code too. `collect` sees the modules the run loads, so `flick check one.fl` gives it
fewer blocks than a whole build. A package whose modules have plugin blocks
is checked again in each build.

### Source checks and refined predicates

`check-structure` receives the structure of a block's original expression and declaration holes, together with the project's lint thresholds. It can report warnings without type wildcards. Generated wrappers do not contribute to these facts. The UI plugin uses it for T036–T038 on `component`.

A refined call carries enum literals or an expression arena with resolved references. The refined plugin recognizes `Checks.*` and evaluates supported local predicates; an unknown result remains a runtime check. The host supplies no predicate verdict. Protocol 0.2.0 changes the WIT interface and invalidates expansion caches; rebuild external plugins with the matching SDK.
