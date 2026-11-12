---
title: "Flick grammar"
---

This is the surface syntax accepted by `flick check` and `flick fmt`; the
language itself is described in the [specification](../../spec/). Every ```` ```flick ```` block
in this file and in the specification is parsed by the test suite
(`flick-core/tests/corpus.rs`); a block that stops parsing fails the build.

## Lexical structure

A source file is UTF-8 (`FLICK-S001` otherwise). Positions in diagnostics are
UTF-8 byte ranges; layout columns count characters.

### Whitespace and comments

- Spaces separate tokens. Tabs are rejected anywhere outside strings and
  comments (`FLICK-S009`), because layout depends on columns.
- Line breaks are `\n` or `\r\n`.
- `// …` is a line comment, `/// …` a doc comment (used by `flick doc`),
  `/* … */` a block comment; block comments nest.

### Names

| Token | Form | Used for |
| - | - | - |
| name | `[a-z_][A-Za-z0-9_]*` | values, functions, fields, type variables |
| capitalized name | `[A-Z][A-Za-z0-9_]*` | types, constructors, modules, classes, services, errors |
| `_` | alone | wildcard pattern |

Qualified names are written without spaces around `.`: `Users.get`,
`Ui.Event.exhaust`, `Sql.Template`. `user.name` is field access.

### Keywords

```text
module exposing import as let mut rec fun match with try if then else task await
scope provide ignore todo echo type opaque where kind error service class
instance deriving foreign from forall not none true false
in when for yield do break continue
```

Contextual words are ordinary names everywhere except in one position:

- `loop` before `{` and `while` before a condition followed by `do`;
- `unboxed` and `untagged` right before `type` (see `codegen.md`);
- foreign modes `pure`, `effect`, `blocked` and the source
  `global`;
- foreign metadata `arity`, `this`, `catch`, `cancel`, `nullable`,
  `retention`, `optional`, `variadic`, `reviewed` (unknown metadata is
  `FLICK-S011`);
- `sql` before a body, a name and a body, `table` or `migration`: a block of
  the plugin of `std`, which reads its clauses, its columns and its DDL (see
  [sql.md](../sql/));
- `cli` before a string and `{`: a block of the plugin of `std`, which reads
  its clauses (see [command-line applications](../cli-dsl/));
- `codec` before a schema and `{`, `server` before `fn` or a name, and
  `component` before the name of a top-level declaration: blocks of the plugin
  of `std` (see [the codec DSL](../schema/#the-codec-dsl),
  [server functions](../server-fn/) and [ui.md](../ui/#components)). Every
  module has these blocks of `std` without an import;
- `and` between the bindings of a `do` group, `x <- a and y <- b`;
- `effect` before the name of an effect declaration, `tail`, `abort` and
  `control` before an operation of a service or an effect, and `return`
  before the pattern of an arm of `try` (see
  [effects.md](../effects/#effects-and-handlers)).

### Float literals

The grammar of JavaScript `NumericLiteral`, so a literal means exactly the JS
number (see [Schema and standard data types](../../spec/07-schema/)):

- decimal `0`, `42`, `1.5`, exponents `1e-3`, `2E+10`;
- `0x1F`, `0o17`, `0b1010` (prefix letters in either case);
- `_` separates two digits: `1_000_000`, `0xFF_FF`. It cannot start or end a
  digit run, repeat, or touch `.`, `e` or a prefix;
- a BigInt is an integer with suffix `n`: `10n`, `0x1Fn`, no fraction or
  exponent;
- a decimal number has no leading zero: `010` (legacy octal) and `09` are
  errors; write `0o10`;
- unlike JavaScript, `.5` and `5.` are errors: `.` is field access. Write
  `0.5` and `5.0`.

A malformed literal is one `FLICK-S006` diagnostic; it never splits into more
tokens.

### String literals

- `"…"` on one line, with escapes `\n \t \r \\ \" \0` and `\u{…}` with 1 to 6
  hex digits naming a Unicode scalar value (no surrogates, at most `10FFFF`).
  `\0` cannot be followed by a digit. Any other escape is `FLICK-S005`.
- Raw strings `r"…"`, `r#"…"#`, `r##"…"##`… contain no escapes, may span lines
  and end at `"` followed by the same number of `#`.
- Multi-line strings open with `"""` followed by a line break and end at the
  next `"""`; escapes work as in `"…"`. The value drops the first line break,
  drops the last line if it holds only spaces before the closing `"""`, and
  removes the common indentation of the non-blank lines (the closing line
  counts). Escapes are decoded after that, so `\u{20}` is content, not
  indentation.
- A hole `\{e}` in a one-line string `"…"` puts `display e` there:
  `"n = \{n + 1}"` is `"n = " ++ display (n + 1)`. `display` is the method of
  the Prelude class `Display`: a `String` is itself, without the quotes `show`
  adds, and `Int`, `Float` and `Bool` are spelled as `show` spells them. A
  type without an instance is `FLICK-C001`. The expression of a hole ends on
  the line of its string (`FLICK-S004` otherwise) and may hold braces and
  strings with holes of their own. Raw strings have no holes, `\{` in a
  multi-line string is `FLICK-S005`, and neither a string pattern nor the
  message of `todo` has them (`FLICK-S007`, `FLICK-S005`). The
  release optimizer joins the constant parts: `"\{2 + 3} of \{"x"}"` compiles
  to `"5 of x"`.

Line breaks inside any string value are `\n`, also in `\r\n` sources.
Expressions made only of literals are evaluated at compile time with JS
semantics (see [Evaluating literal expressions](../../spec/06-typescript-and-ffi/#evaluating-literal-expressions)): `60 * 60 * 24`,
`"a" ++ "b"`, `not false`.

```flick
let examples =
  [
    "tab\t and \u{1F600}",
    r"C:\temp",
    r#"say "hi""#,
    "sum: \{2 + 3}",
    """
    first line
      indented line
    """,
  ]
```

### Operators and punctuation

```text
( ) [ ] { } , ; : . .. ..< = -> <- => | ^ _ ?
|> <| >> << || && == != < <= > >= ++ + - * / % ** not
```

## Operator table

There are no user-defined operators. From weakest to strongest:

| Level | Operators | Associativity |
| - | - | - |
| 0 | `<\|` | right; does not mix with `\|>`: `x \|> f <\| y` is an error |
| 1 | `\|>` | left |
| 2 | `\|\|` | right |
| 3 | `&&` | right |
| 4 | `==` `!=` `<` `<=` `>` `>=` | none: `a < b < c` is an error |
| 4½ | `a..b`, `a..<b`, `a..` | none: `a..b..c` is an error |
| 5 | `++` | right |
| 6 | `+` `-` | left |
| 7 | `*` `/` `%` (`%` is the JS remainder) | left |
| 8 | `**` | right |
| 9 | prefix `-`, `not`, `await`, `ignore`, `echo` | — |
| 9½ | `>>`, `<<` | `>>` left, `<<` right; they do not mix: `f >> g << h` is an error |
| 10 | function application `f x y` | left |
| 11 | `.field`, `Module.name`, postfix `?` | left |

`-x ** 2` is `-(x ** 2)`; `-a * b` is `(-a) * b`; `await f x` is
`await (f x)`; `0..n - 1` is `0..(n - 1)`, and `i < 0..n` compares with the range.
`f <| g <| x + 1` is `f (g (x + 1))`, and `f a <| x` is the call `f a x`;
`List.map f >> List.filter p` composes two applications, and `await f >> g`
is `await (f >> g)`. What the operators mean is in [Numbers and
operators](../types/#numbers-and-operators).
A range is a `Seq Float`: `a..b` counts from `a` up to `b` inclusive
(`Seq.range_to`), `a..<b` stops before `b` (`Seq.range`), and `a..` never
ends (`Seq.from`). Alone in brackets, `[a..b]` is the `List` of the same
numbers; `[a..]` is an error. The number before `..` ends there: `1..5` is
a range, `1.5` a number. Bitwise operations are the functions `Float.bit_*`.

## Layout

Function bodies, match arms and the members of `class`, `instance` and
`service` are delimited by indentation; there is no `in` and the last
expression of a block is its value (see [Direct effectful calls](../../spec/01-direct-calls/)). Braces delimit records and the
blocks of `task`, `scope`, `provide`, `sql`, `ui`, `rec` and plugins.

1. Top-level declarations start at column 1. A line indented further continues
   the current declaration.
2. A *block* opens after `=`, `->`, `then`, `else` and `try`:
   - if the block starts on the next line, its column is the column of its
     first token. A line at that column starts the next statement, a deeper
     line continues the current statement, a shallower line ends the block;
   - if the block starts on the same line, it holds one statement; it
     continues on lines indented deeper than the line it started on (and
     deeper than any enclosing brace block), and ends at any other line;
   - after `then` and `else`, `{` opens a brace block of statements, as after
     `do` and `loop`; a record there goes in parentheses (`FLICK-T074`).
3. A line that starts at the block column with an infix operator other than
   `-`, with `|`, `then`, `else`, `with`, or with a closing delimiter continues
   the statement instead of starting a new one. This is what lets `|>` and
   `<|` pipelines, `match` arms and `else` branches line up with their
   expression.
4. Inside `{ … }` of a record, record type or brace block the items align on
   the first item, which may follow `{` on the same line; items are separated
   by line breaks or `;`. `;` separates statements only inside braces.
5. Inside `( … )`, `[ … ]` and row braces line breaks do not matter, but a
   line starting left of the enclosing block's column ends the bracket (so a
   missing `)` does not swallow later declarations).
6. A closing delimiter may be dedented below the block it closes.
7. `else if` on one line continues the chain without opening a block.
8. In the body of a block that asks for it, as `ui { … }` does, the lines
   that continue a call are its arguments, one whole expression each, and
   the deeper lines under one of those are that line's own: `div props`,
   then a line for each child. A line that rule 3 or 5 makes a continuation
   stays one. Elsewhere such lines are the arguments token by token, as
   rule 2 has them.

```flick
# type UserId = UserId String
# type alias User = { name : String }
# type alias UserView = { title : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# service Log =
#   info : String ->{} Unit
# to_view : User -> UserView
# let to_view user = { title = user.name }
#
load_view : UserId ->{Users, Log; NotFound} UserView
let load_view id =
  let user = Users.get id
  let view = to_view user
  Log.info view.title
  view

classify : Float -> String
let classify n =
  if n < 0 then
    "negative"
  else if n == 0 then
    "zero"
  else
    "positive"

total : List Float -> Float
let total xs =
  xs
  |> List.filter (fun x -> x > 0)
  |> List.sum
```

A line whose column matches no enclosing block, or a body expected on the next
line that is not indented, is one `FLICK-S009` diagnostic at that line:

```text
let between x =
  match x with
  | A ->
      foo
     bar        // FLICK-S009: column 6 continues nothing here
```

## Declarations

```text
file        = [module_header] { import } { decl | module_application }
module_header = "module" ModulePath ["exposing" exposing]
import      = "import" ModulePath ["as" Con] ["exposing" exposing]
exposing    = "(" item { "," item } [","] ")"
item        = name | Con | Con "(..)" | ".." | "plugin" name   // see plugins.md
ModulePath  = Con { "." Con }

decl        = signature | let | rec_group | type_decl | kind_decl | error_decl
            | service | class | instance | foreign | module_contract | inline_module
            | head { head_arg } ("{" { token } "}" | { token })   // a plugin's, to the next decl
inline_module = "module" Con ["exposing" exposing] "=" indented { import | decl }
module_contract = "type" "module" Con "=" indented { module_member }
module_application = "module" Con "=" ModulePath "(" ModulePath { "," ModulePath } ")"
signature   = name ":" type
let         = "let" ["rec"] (name { atom_pattern } | pattern) [":" type] "=" block
            | "let" "assert" pattern [":" type] "=" block ["as" String]
            | "let" "mut" name [":" type] "=" block            (in a block only)
rec_group   = "rec" "{" { signature | let } "}"
type_decl   = ["opaque"] "type" Con { param } ["=" variants] [deriving]
            | ["opaque"] "type" "alias" Con { param } "=" type [deriving]
            | "refined" "type" Con { param } "=" type "where" expr [deriving]    (a declaration of std's plugin)
            | ["opaque"] ("unboxed" | "untagged") "type" Con { param } "=" variants [deriving]
variants    = ["|"] variant { "|" variant }
variant     = Con { capture } ["=" literal] [":" type]
            // fields are atomic types; a literal only in an enum; `: type`, what a
            // constructor of a GADT builds, only in a `type` and without a literal
literal     = string | ["-"] number
param       = name | "(" name ":" kind ")"
deriving    = "deriving" Con { "," Con }
kind_decl   = "kind" Con "=" ["|"] Con { "|" Con }
error_decl  = "type" "error" Con { param } ["=" variant] [deriving]
service     = ("service" | "effect") Con { param } [ "=" operations ]
operations  = block of: [ "tail" | "abort" | "control" ] signature | let | type_decl
class       = "class" [context "=>"] Con { param } ["|" fundep { "," fundep }] ["=" members]
fundep      = { name } "->" { name }
instance    = "instance" [context "=>"] type ["=" members]
members     = block of: signature | let | type_decl
foreign     = "foreign" [mode] name "from" ("global" String | String String) [metadata]
            | "foreign" "blocked" name String
mode        = "pure" | "effect"
metadata    = indented lines: meta_name { atom }
```

`module_application` is a file-level declaration. Writing it inside an
`inline_module` reports `FLICK-R019`; a `module_contract` is allowed there.

The right-hand side of `type` is always a list of variants, so
`type Wrapper = Wrap Float` is an ADT with one constructor; a transparent
alias, a record type included, is written `type alias`. `alias` is a keyword
only right after `type`. A field of a variant is an atomic type, as in a
constructor pattern: `Some (List a)`, `Handler (a -> b)`, and
`Circle (Point, Float)` has one tuple field where `Circle Point Float` has
two. Writing `of` before the fields is `FLICK-S010`.

`type error E` declares an error; its one constructor is named `E` too, and an
error with fields is `type error E = E T …`. A constructor named otherwise is
`FLICK-S007`.
Every named function has its full signature right above its `let` (see [Direct effectful calls](../../spec/01-direct-calls/)),
except an instance method, whose signature comes from its class; the
parser accepts a signature without a definition, and later phases check the
pairing.

```flick
module App.Shapes exposing (Shape(..), area)

import App.Geometry exposing (Point)
import Effect.Log as Log
# import File exposing (Path)
# import JsError as FsError

type Shape =
  | Circle Point Float
  | Rect { origin : Point; size : Point }
  | Empty
  deriving Eq
# type alias FsError = FsError.JsError
# area : Shape -> Float
# let area _ = 0.0

opaque type alias UserId = String

refined type Port = Float where fun n -> n > 0 && n < 65536
  deriving Eq, Ord

kind Nullability = NonNull | Nullable

type error NotFound = NotFound UserId

service Clock =
  now : Unit ->{} Float

class Eq a => Hash a =
  hash : a -> Int

class Collection c a | c -> a =
  empty : c

instance Show Shape =
  let show shape =
    match shape with
    | Circle _ _ -> "circle"
    | _ -> "other"

read_text : Path ->{Async; FsError} String
foreign read_text from "./fs.js" "readText"
  arity 1
  catch FsError.from_js
  cancel signal

rec {
  is_even : Float -> Bool
  let is_even n = if n == 0 then true else is_odd (n - 1)

  is_odd : Float -> Bool
  let is_odd n = if n == 0 then false else is_even (n - 1)
}
```

## Types, rows and kinds

```text
type        = "forall" name { name } "." type
            | union "=>" type                  // constraint(s): `Show a =>`, `(Show a, Eq a) =>`
            | union ["->" [effects] type]      // right associative
effects     = "{" [row] [";" [row]] "}"     // written `->{` with no space
row         = union { "," union } ["|" union]  // `none`, `r`, `Users, Log | r`
union       = app { "+" app }                  // row or capture union
app         = capture { capture }
capture     = atom { "^" (name | "(" type ")" | "{" … "}") }
atom        = name | Con { "." Con } | "none" | "(" ")" | "(" type ")"
            | "(" type "," type { "," type } ")"
            | "{" field ":" type { (";" | line break) field ":" type } ["|" union] "}"
            | "{" [union { "," union }] ["|" union] "}"
```

- `a -> b` is pure; `a ->{r; e} b` is a runtime call needing services `r` and
  failing with errors `e`; `a ->{} b` is a runtime call with both rows empty,
  never the same as `->` (see [Specification](../../spec/), [Values of computations and their execution](../../spec/02-computations/)). `-> {` with a space is an arrow to a record
  type.
- `none` is the empty row literal. An arrow side left out is empty too:
  `->{; E}` is `->{none; E}`, `->{R}` is `->{R; none}` (see
  [effects.md](../effects/#empty-rows-none)). `{Users, Log}` is a closed row, a
  trailing `| r` opens it.
- `T^c`, `T^{s}`, `T^(c + d)` qualify a type with the captures it holds; `^`
  binds tighter than application, so `List a^c` is `List (a^c)`.
- Kinds use the same grammar: `Type`, `Type -> Type`, `ServiceRow`.

```flick
# type alias User = { name : String }
#   deriving Schema
# type alias Settings = { theme : String }
#   deriving Schema
# type error NotFound = NotFound
# type error SaveError = SaveError
# service Users =
#   get : String ->{; NotFound} User
# service Log =
#   info : String ->{} Unit
# import App.Ui exposing (SaveProps)
#
decode_pair :
  (forall a. Schema a -> Unknown ->{r; SchemaError} a) ->
  Unknown -> Unknown ->{r; SchemaError} (User, Settings)
# let decode_pair decode user settings = (decode User.schema user, decode Settings.schema settings)

open_rows : a ->{Users, Log | r; NotFound | e} b
# let open_rows _ = todo

fields : forall fields. { name : String | fields } -> { name : String }
# let fields record = { name = record.name }

save_with :
  SaveProps c -> (Eff {Users} {SaveError} Unit)^c
# let save_with _ = task { () }
```

## Expressions

```text
block       = statement { line break statement }
statement   = "let" … | signature | name "=" block | expr
expr        = "fun" atom_pattern { atom_pattern } "->" block
            | "loop" "{" block "}"
            | "while" expr "do" block
            | "break" [expr] | "continue"
            | "match" expr "with" arm { arm }
            | "try" block "with" handler_arm { handler_arm }
            | "if" expr "then" block [ "else" (if_expr | block) ]
            | operator expression over application (see the operator table)
arm         = "|" or_pattern ["when" expr] "->" block
handler_arm = arm | "|" "return" or_pattern "->" block
            // an operation's arm is `| E.op p… -> …`, a pattern whose path ends in a name
application = postfix { postfix }
postfix     = atom { "." name | "?" }           // no space before either
atom        = name | Con { "." Con } ["." name] | Float | BigInt | String
            | "true" | "false" | "()" | "(" expr ")" | "(" expr ":" type ")"
            | "(" expr "," expr { "," expr } ")" | "[" [expr { "," expr } [","]] "]"
            | "{" "}" | "{" field "=" block { ";" | line break … } "}"
            | "{" expr "with" field "=" block { … } "}"
            | "task" "{" block "}" | "ui" "{" block "}"
            | "do" "{" do_stmt { ";" | line break … } "}"
            | "for" ("{" clause_list "}" | clause_list) ("yield" expr | "do" block)
            | "scope" [name] "{" block "}" | "provide" expr "{" block "}"
            | head { head_arg } ["{" { token } "}"]
                                            // a head an import exposes as `plugin head`, with an
                                            // argument or a body; `cli` with its body
head_arg    = name | Con | Number | String | "." | "(" { token } ")"   // on the head's line
do_stmt     = bind { "and" bind } | "if" expr | statement
bind        = or_pattern "<-" expr
clause_list = for_clause { ";" | line break | before "if" … for_clause }
for_clause  = or_pattern "<-" expr | "if" expr | let_decl
clause      = ("from" | "join" | "left_join") name "in" expr
            | clause_word expr { "," expr }
```

`do { … }` sequences its statements through the Prelude's `Monad`:
`p <- e` is `flat_map (fun p -> rest) e`, a bare expression is bound to
nothing, `if c` without `then` is `if c then rest else nothing` of
`Alternative`, `let` scopes over the rest, and the last statement is an
expression of the block's monad. The pattern after `<-` must be irrefutable,
and the rest of the block is a pure function: a runtime call there is
`FLICK-E001` (see [`do`, `for` and comprehensions](../../spec/04-higher-order/#do-for-and-comprehensions)).
`p <- a and q <- b` binds both independently through `map2` of
`Applicative`: neither value sees `p` or `q`, and a group followed only by
`pure e` needs no `Monad`. `and` is a word only there; elsewhere it is a name.
`for { x <- xs; if p x } yield e` is a lazy `Seq`: one generator runs
the clauses on each iteration. `x <- source` takes the elements of an
`Iterable` source (`List`, `Seq`) and skips those the pattern does not match,
`if c` skips the element unless `c`, and `let` scopes over the rest; the
clauses make Sync calls at most (`Atom.Read`), reading the services of the
stage that builds the sequence. `[for … yield e]`, a `for` alone in brackets, is the strict
`List` of the same elements, built with a loop. Without braces the clauses
are separated by `;`, and a guard's `if` starts a new one. `do body` in place
of `yield e` runs the body, a `Unit` block, for each element right there:
the clauses and the body may make runtime calls, which join the rows of the
function, and the loop is a `for…of` inside its computation.
`<-` outside a `do` or `for` block is `FLICK-S013`; a comparison with a negative number
is written `a < -1`.

Postfix `e?`, written right after its operand, unwraps it. On a
`Result (Errors e) a` in a runtime function it is `from_result e`: the value,
or a failure with the error, which joins the function's errors. On an
`Option a` in a function returning an `Option` it is the value of `Some`, and
a `None` returns `None` from the function at once. Elsewhere it is an error
(inside `task`, `scope`, `provide`, `try` and `for`, which run on their own,
too). `f x?` is `f (x?)`; write `(f x)?`.

`todo` (optionally followed by a string) is an expression of any type, and
`echo` without an operand is a function, as in `x |> echo`. `_` is a type
only in a signature or annotation. `if` is an expression; without `else` it is `Unit`, and so must be its `then` branch (an `else ()` gets the warning `FLICK-T028`). A branch in braces holds statements: `if a > b then { low = b; high = a }`. `ignore e` discards a non-`Unit`
result of an intermediate statement (see [Direct effectful calls](../../spec/01-direct-calls/)). `await e` runs a stored `Eff` once in
the current fiber; `task { … }` builds a cold computation (see [Values of computations and their execution](../../spec/02-computations/)). In
`provide e { … }` the `{` ends the head expression.
A plugin block keeps its arguments and body as tokens for its plugin, which
reads them (see [plugins](../plugins/)). `cli "name" { … }` is the block of
the plugin of `std`: it reads the clauses, and checks aliases on an `arg` and
duplicate names, while the types of `Cli` check the options (see [CLI eDSL:
flags, arguments and subcommands](../../spec/08-type-programming-and-edsl/#cli-edsl-flags-arguments-and-subcommands)). A `///` comment right above a `flag`, `arg` or
`subcommand` clause is its help text.

```flick ignore several statements shown without the declarations around them
load_twice : UserId ->{Users; NotFound} (User, User)
let load_twice id =
  let request = task { Users.get id }
  let first = await request
  let renamed = { first with name = "Ada" }
  (renamed, await request)

find_optional : UserId ->{Users; DbUnavailable} Option User
let find_optional id =
  try
    Some (find_user id)
  with
  | NotFound _ -> None

read_config : Path ->{FileSystem; OpenError, ReadError} String
let read_config path =
  scope {
    let file = File.open_scoped path
    File.read_all file
  }

balances : Sql.Template Pg { user_id : UserId } BalanceRow
let balances =
  sql params {
    from user in Users
    left_join account in Accounts
      on account.user_id == user.id
    where user.id == params.user_id
    select { name = user.name; balance = account.balance }
  }

deploy : Cli.Command {Platform} none
let deploy =
  cli "deploy" {
    description "Deploy the current build"
    /// Print the plan without applying it
    flag dry_run "d" : Bool
    flag region : Option String
    arg targets : NonEmpty String
    handle fun { dry_run; region; targets } -> Deploy.run dry_run region targets
  }
```

## Patterns

```text
or_pattern   = pattern { "|" pattern }
pattern      = Con { "." Con } { atom_pattern } | atom_pattern
atom_pattern = "_" | name | Con | ["-"] Float | ["-"] BigInt | String | "true" | "false"
             | "()" | "(" or_pattern ")" | "(" or_pattern ":" type ")"
             | "(" or_pattern "," or_pattern { "," or_pattern } ")"
             | "[" [list_item { "," list_item }] "]"
list_item    = or_pattern | "..." (name | "_")
             | "{" field ["=" or_pattern] { ";" | line break … } "}"
```

A record pattern field without `= pattern` binds a variable named like the
field. `...rest` may only be the last item of a list pattern; it binds the
rest of the list. Or-patterns and guards follow Rust's rules (see [Variants, guards and or-patterns](../../spec/06-typescript-and-ffi/#variants-guards-and-or-patterns)): every alternative binds the same names, a name is bound at
most once per pattern or per function's parameters (`FLICK-S012`), and a
guard `when e` covers the whole arm. A `|` at the start of a line inside an
arm's pattern continues the or-pattern; after `->` it starts the next arm.

```flick
# type alias Point = { x : Float; y : Float }
# type Shape =
#   | Circle Point Float
#   | Rect Float Float
#   | Labeled { name : String }
#   | Empty
#
describe : Shape -> String
let describe shape =
  match shape with
  | Circle _ _ | Empty -> "round or nothing"
  | Rect width height when width == height -> "square"
  | Rect _ _ -> "rectangle"
  | Labeled { name = "origin" | "zero" } -> "special"
  | _ -> "other"
```

## Constructs Flick does not have

A `do` block laid out without braces, `let*` binding operators and JSX `<…>`
each produce one `FLICK-S010` diagnostic naming the Flick replacement
(`do { … }`, direct calls with `let`, `task`/`await`, `ui { … }`), not a
generic parse error.

## Error recovery

Each lexical problem is one diagnostic and one token. After a syntax error the
parser skips the rest of the statement and resumes at the next statement or
declaration, so independent errors are all reported and one error does not
cascade into others in the same statement.

## Formatting

`flick fmt` writes the canonical layout of a file, as `gleam format` does:
the result depends on the code and the line width (100 columns) only, not on
how the source was laid out, apart from the few choices listed below. Literals,
multi-line strings and comments are copied verbatim. The formatter drops a
redundant outer `forall` from a nonrecursive `let` signature, including a method
of an `instance`, when its variables remain implicitly quantified; it keeps one
needed by recursion, type shadowing or a comment. Other rewrites are separators that change nothing: `;` between
statements becomes a line break, a broken bracket gets a trailing `,` and a
broken record a `;` after every field, a broken variant list starts with
`|`, and empty rows of an arrow are dropped, which are the same type:
`->{none; E}` becomes `->{; E}`, `->{R; none}` becomes `->{R}`, and
`->{none; none}` becomes `->{}`. `Eff none none a` stays as written. The formatter re-parses
each declaration and the whole file and refuses with `FLICK-P003` if the AST
or the comments would change.

Indentation is two spaces per level. A construct stays on one line when it
fits; otherwise it breaks, outermost construct first:

- a value after `=` (`let`, record fields, type aliases), after `->` (`fun`,
  match arms) or after `<-` goes to the next line, one level in. Records,
  lists, tuples, `fun` and blocks in braces (`task`, `ui`, `scope`,
  `provide`, `do`, `for`, `sql`, a plugin's) stay after the `=` and break
  inside instead: `let config = {` … `}`. Several statements always start on
  the next line, and so do `match`, `try` and an `if` that does not fit;
- statements go one per line; a block in braces with one expression may stay
  on one line, `task { load id }`;
- an application puts its arguments one per line, one level in. When its last
  argument is a function in parentheses, a list, a record, a tuple or a block
  in braces, and everything before that argument's first break fits, the call
  stays on its line and only the argument breaks:
  `Ref.modify counter (fun current ->`, the body, then `)` on its own line. A
  call in parentheses stays hugged by them;
- an operator chain breaks before each operator of its weakest precedence,
  continuations one level in; a pipeline of `|>` or `<|` breaks one step per
  line, its operators under the first operand when the pipeline starts its
  line;
- `if c then a else b` stays on one line or puts every branch on lines of its
  own; `match` and `try` put each arm on its own line, at the column of
  `match`. Before a `,`, where the parser takes no block on lines of its own,
  the last branch or arm stays on its line;
- lists, tuples and `exposing (…)` put one item per line followed by `,`;
  records, record types, patterns and updates (`base with` on its own line)
  put one field per line followed by `;`, a row tail `| r` last;
- a signature puts its type on the next line, then `forall a.`, a context and
  each parameter of a function type on lines of their own; elsewhere a
  function type breaks after its arrows, one level in, `->{r; e}` staying
  whole;
- variants stay on one line when they fit, else one per line after `|`;
  `deriving` takes its own line, one level in, except after the `}` of a record
  that broke; members of classes, instances, services and `rec`, the metadata
  of a `foreign` binding and the columns of a `sql table` always go one per
  line; in `sql`, `on` stays after its `join` when it fits.

What the author decides, as in Gleam: a trailing `,` or `;` (`[a, b,]`,
`{ a = 1; }`), or a blank line between items, keeps a bracket broken one item
per line; a list of constants written several per line with a trailing `,`
is filled, as many per line as fit; a pipeline written with a step on its own
line, its `|>` or `<|` at the start or at the end of a line, keeps one step
per line, each operator at the start of its line. Blank lines between declarations, statements,
members, arms, fields and items are kept, a run of them as one. A comment on
its own line stays on its own line before what follows it, a comment that
ends a line stays at the end of that line, and the construct around it
breaks. A declaration holding a line comment where the layout has no place for
one is left as written.

`flick fmt --check` exits with `1` and reports `FLICK-P002` for each file that
would change; `flick fmt --stdin --stdin-filepath <file>` reads the source
from stdin and writes the result to stdout for editors.
