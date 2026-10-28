---
title: "Keywords"
---

Every reserved word of Flick, what it introduces and where it is described in
full. A keyword cannot be used as a name. The words that are keywords only in
one position (`alias`, `unboxed`, `component`, the
foreign modes, the SQL and CLI clause words) are listed in [grammar.md](../grammar/#keywords).

## Modules

| Keyword | Meaning | Example |
| - | - | - |
| `module` | names the module of the file | `module App.Main exposing (run)` |
| `exposing` | the names a module exports, or an import brings in unqualified | `import App.Domain exposing (User, greet)` |
| `import` | uses another module | `import Effect.Log as Log` |
| `as` | a local name for an imported module; the Flick type of a `sql table` column | `import Effect.Log as Log` |

See [modules.md](../modules/).

## Declarations

| Keyword | Meaning | Example |
| - | - | - |
| `let` | binds a value or a function, at the top level or in a block | `let total = a + b` |
| `rec` | `let rec` lets a function call itself; `rec { … }` groups mutually recursive functions | `let rec go n = …` |
| `assert` | `let assert` binds a pattern that may not match, and fails when it does not | `let assert [x, ..._] = xs as "no items"` |
| `type` | declares an ADT, `type alias` a transparent alias | `type Shape = Circle Float \| Empty` |
| `opaque` | hides a type's representation outside its module | `opaque type alias UserId = String` |
| `refined` | a type whose values pass a predicate | `refined type Port = Float where fun n -> n > 0` |
| `where` | the predicate of a `refined` type; a filter in `sql { … }` | `where user.id == params.id` |
| `kind` | declares a kind with its type-level constructors | `kind Nullability = NonNull \| Nullable` |
| `error` | `type error` declares an expected error with a nominal identity | `type error NotFound = NotFound UserId` |
| `service` | declares the operations of a service, looked up at run time | `service Clock = now : Unit ->{} Float` |
| `class` | declares a type class | `class Named a = name : a -> String` |
| `instance` | implements a class for a type | `instance Named User = let name u = u.name` |
| `deriving` | asks the compiler for instances | `deriving Eq, Show, Schema` |
| `foreign` | binds a JavaScript value with a Flick signature | `foreign async read_text from "./fs.js" "readText"` |
| `from` | the source of a `foreign`; a table in `sql { … }` | `from user in Users` |
| `forall` | quantifies type variables explicitly | `keep : forall s. Path ->{Scope s} File s` |

See [types.md](../types/), [classes.md](../classes/), [effects.md](../effects/)
and [ffi.md](../ffi/).

## Expressions

| Keyword | Meaning | Example |
| - | - | - |
| `fun` | an anonymous function | `fun x -> x + 1` |
| `if` `then` `else` | a conditional expression; without `else` it is `Unit` | `if n == 0 then 1 else n` |
| `match` `with` | pattern matching | `match shape with \| Circle r -> r` |
| `when` | a guard on a match arm | `\| Rect w h when w == h -> "square"` |
| `with` | also the handlers of `try` and a record update | `{ user with name = "Ada" }` |
| `try` | handles the expected errors of its body | `try find id with \| NotFound _ -> None` |
| `not` | boolean negation; `not null` on a `sql table` column | `not done` |
| `true` `false` | the two `Bool` values | `if ok then true else false` |
| `none` | the empty row in a type | `a ->{none; NotFound} b` |
| `in` | binds a row in a SQL `from` or `join` clause | `join account in Accounts` |
| `do` | statements sequenced through `Monad`: `x <- e` binds, `if c` guards | `do { x <- left; y <- right; pure (x, y) }` |
| `for` `yield` | a lazy `Seq` of the elements `yield` gives, one generator; `[for …]` is a strict `List`; `for … do body` runs the body for each element | `[for x <- xs if x > 0 yield x * 2]` |

## Effects and resources

| Keyword | Meaning | Example |
| - | - | - |
| `task` | builds an `Eff r e a` from its block without running it | `task { Users.get id }` |
| `await` | runs an `Eff` once in the current fiber and gives its result | `await request` |
| `scope` | runs its block in a fresh region and releases what the block acquired when it ends | `scope { File.read_all (File.open_scoped path) }` |
| `provide` | runs its block with the services a layer builds | `provide live { run user }` |
| `ignore` | discards the value of an intermediate statement | `ignore (Store.save user)` |

`ignore` evaluates its operand and throws the value away. An `Eff` it
discards is not run: write `ignore (await t)` to run `t` and drop its result.

See [effects.md](../effects/#task-and-await) and [regions.md](../regions/).

## Debugging

| Keyword | Meaning | Example |
| - | - | - |
| `todo` | code not written yet: any type, fails when it runs | `todo "parse the header"` |
| `echo` | writes a value and its place to stderr, and returns it | `echo (total rows)`, `rows \|> echo` |

Both check anywhere and leave a warning (`FLICK-T022`, `FLICK-T023`);
`flick pack` refuses a package that still has them (`FLICK-P024`). `echo`
is not an effect: it is for debugging, like a breakpoint. Both stay usable
as record labels: `{ echo = … }`.

## Embedded languages

| Keyword | Meaning | Example |
| - | - | - |
| `schema` | the `Schema` of a type that derives it; `schema T` | `Codec.json (schema Shape)` |
| `codec` | a codec built field by field | `codec Codec.json { name = … }` |
| `sql` | a typed SQL query, or `sql table` for a table | `sql { from u in Users select u }` |
| `ui` | a UI block | `ui { … }` |
| `cli` | a command-line interface | `cli "deploy" { flag dry_run : Bool … }` |

See [schema.md](../schema/), [sql.md](../sql/), [ui.md](../ui/) and
[cli-dsl.md](../cli-dsl/).

## In the editor

Hovering a keyword that opens an expression shows the type of the whole
expression, the type an ML language server shows for an expression, together
with what the keyword does:

| Hover on | Shows |
| - | - |
| `task` | `task { … } : Eff {Store} {StoreError} Float`: the computation the block builds |
| `scope` | `scope { … } : Float`: the value the block returns once its resources are released |
| `provide`, `try` | the value of the block, with the services or errors it removes |
| `match`, `if` | `match … with : String`: the value of the arm or branch taken |
| `fun` | `fun … -> … : Float -> Float`: the function |
| `sql`, `codec`, `cli`, `ui`, `schema` | the value the block builds, such as `sql { … } : Template Pg {} { label : String }` |
| `await` | `await … : Float`: the result, with the `Eff` it runs |
| `ignore` | `ignore : Eff {Store} {StoreError} Float -> Unit`: the type of what is discarded, with a warning when it is an `Eff` that is not run |

Other keywords show nothing on hover; the names around them show their types.
