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
  `/* … */` a block comment; block comments nest. The same comments work inside
  `sql { … }`.

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
module exposing import as let rec fun match with try if then else task await
scope provide ignore todo echo type opaque refined where kind error service class
instance deriving foreign from forall not none true false schema codec
sql ui cli in when for yield do
```

Contextual words are ordinary names everywhere except in one position:

- `unboxed` and `untagged` right before `type` (see `codegen.md`);
- foreign modes `pure`, `sync`, `async`, `effect`, `blocked` and the source
  `global`;
- foreign metadata `arity`, `this`, `catch`, `cancel`, `nullable`,
  `retention`, `optional`, `variadic`, `reviewed` (unknown metadata is
  `FLICK-S011`);
- SQL clause words inside `sql { … }`: `join`, `left_join`, `on`, `group_by`,
  `having`, `order_by`, `limit`, `offset`, `select`, `union`, `union_all`,
  `insert_into`, `values`, `update`, `set`, `delete_from`, `returning` (plus the
  keywords `from` and `where`). Inside a SQL block such a word ends a function
  application, so a clause may continue on the next line;
- `table` right after `sql` at the top level, and `via` after the type of a
  column of a `sql table` (see [sql.md](../sql/));
- CLI clause words at the start of a clause inside `cli { … }`: `description`,
  `flag`, `arg`, `subcommand`, `handle`;
- `and` between the bindings of a `do` group, `x <- a and y <- b`;
- `component` before the name of a top-level declaration (see
  [ui.md](../ui/#components)).

### Float literals

The grammar of JavaScript `NumericLiteral`, so a literal means exactly the JS
number (see [Schema and standard data types](../../spec/09-schema/)):

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

There is no interpolation.

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

Line breaks inside any string value are `\n`, also in `\r\n` sources.
Expressions made only of literals are evaluated at compile time with JS
semantics (see [Evaluating literal expressions](../../spec/08-typescript-and-ffi/#evaluating-literal-expressions)): `60 * 60 * 24`,
`"a" ++ "b"`, `not false`.

```flick
let examples =
  [
    "tab\t and \u{1F600}",
    r"C:\temp",
    r#"say "hi""#,
    """
    first line
      indented line
    """,
  ]
```

### Operators and punctuation

```text
( ) [ ] { } , ; : . .. ..< = -> <- => | ^ _ ?
|> || && == != < <= > >= ++ + - * / % ** not
```

## Operator table

There are no user-defined operators. From weakest to strongest:

| Level | Operators | Associativity |
| - | - | - |
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
| 10 | function application `f x y` | left |
| 11 | `.field`, `Module.name`, postfix `?` | left |

`-x ** 2` is `-(x ** 2)`; `-a * b` is `(-a) * b`; `await f x` is
`await (f x)`; `0..n - 1` is `0..(n - 1)`, and `i < 0..n` compares with the range.
A range is a `Seq Float`: `a..b` counts from `a` up to `b` inclusive
(`Seq.range_to`), `a..<b` stops before `b` (`Seq.range`), and `a..` never
ends (`Seq.from`). Alone in brackets, `[a..b]` is the `List` of the same
numbers; `[a..]` is an error. The number before `..` ends there: `1..5` is
a range, `1.5` a number. Bitwise operations are the functions `Float.bit_*`.

## Layout

Function bodies, match arms and the members of `class`, `instance` and
`service` are delimited by indentation; there is no `in` and the last
expression of a block is its value (see [Direct effectful calls](../../spec/03-direct-calls/)). Braces delimit records and the
blocks of `task`, `scope`, `provide`, `sql`, `ui`, `cli`, `codec` and `rec`.

1. Top-level declarations start at column 1. A line indented further continues
   the current declaration.
2. A *block* opens after `=`, `->`, `then`, `else` and `try`:
   - if the block starts on the next line, its column is the column of its
     first token. A line at that column starts the next statement, a deeper
     line continues the current statement, a shallower line ends the block;
   - if the block starts on the same line, it holds one statement; it
     continues on lines indented deeper than the line it started on (and
     deeper than any enclosing brace block), and ends at any other line.
3. A line that starts at the block column with an infix operator other than
   `-`, with `|`, `then`, `else`, `with`, or with a closing delimiter continues
   the statement instead of starting a new one. This is what lets `|>`
   pipelines, `match` arms and `else` branches line up with their expression.
4. Inside `{ … }` of a record, record type or brace block the items align on
   the first item, which may follow `{` on the same line; items are separated
   by line breaks or `;`. `;` separates statements only inside braces.
5. Inside `( … )`, `[ … ]` and row braces line breaks do not matter, but a
   line starting left of the enclosing block's column ends the bracket (so a
   missing `)` does not swallow later declarations).
6. A closing delimiter may be dedented below the block it closes.
7. `else if` on one line continues the chain without opening a block.

```flick
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
file        = [module_header] { import } { decl }
module_header = "module" ModulePath ["exposing" exposing]
import      = "import" ModulePath ["as" Con] ["exposing" exposing]
exposing    = "(" item { "," item } [","] ")"
item        = name | Con | Con "(..)" | ".."
ModulePath  = Con { "." Con }

decl        = signature | let | rec_group | type_decl | kind_decl | error_decl
            | service | class | instance | foreign | sql_table | component
signature   = name ":" type
let         = "let" ["rec"] (name { atom_pattern } | pattern) [":" type] "=" block
            | "let" "assert" pattern [":" type] "=" block ["as" String]
rec_group   = "rec" "{" { signature | let } "}"
component   = "component" name atom_pattern "=" block
type_decl   = ["opaque"] "type" Con { param } ["=" variants] [deriving]
            | ["opaque"] "type" "alias" Con { param } "=" type [deriving]
            | "refined" "type" Con { param } "=" type "where" expr [deriving]
            | ["opaque"] ("unboxed" | "untagged") "type" Con { param } "=" variants [deriving]
variants    = ["|"] variant { "|" variant }
variant     = Con { capture }                   // fields are atomic types
param       = name | "(" name ":" kind ")"
deriving    = "deriving" Con { "," Con }
kind_decl   = "kind" Con "=" ["|"] Con { "|" Con }
error_decl  = "type" "error" Con { param } ["=" variant] [deriving]
service     = "service" Con { param } [ "=" members ]
class       = "class" [context "=>"] Con { param } ["|" fundep { "," fundep }] ["=" members]
fundep      = { name } "->" { name }
instance    = "instance" [context "=>"] type ["=" members]
members     = block of: signature | let | type_decl
foreign     = "foreign" mode name "from" ("global" String | String String) [metadata]
            | "foreign" "blocked" name String
mode        = "pure" | "sync" | "async" | "effect"
sql_table   = "sql" "table" Con String "{" column { column } "}"
column      = name ":" { name | "not" } "as" type ["via" expr]
metadata    = indented lines: meta_name { atom }
```

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
Every named function has its full signature right above its `let` (see [Direct effectful calls](../../spec/03-direct-calls/)),
except an instance method, whose signature comes from its class; the
parser accepts a signature without a definition, and later phases check the
pairing.

```flick
module App.Shapes exposing (Shape(..), area)

import App.Geometry exposing (Point)
import Effect.Log as Log

type Shape =
  | Circle Point Float
  | Rect { origin : Point; size : Point }
  | Empty
  deriving Eq, Show

opaque type alias UserId = String

refined type Port = Float where fun n -> n > 0 && n < 65536
  deriving Eq, Hash

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

read_text : Path ->{; FsError} String
foreign async read_text from "./fs.js" "readText"
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
effects     = "{" "}" | "{" [row] ";" [row] "}" // written `->{` with no space
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
  never the same as `->` (see [Decisions of the first version](../../spec/02-decisions/), [Values of computations and their execution](../../spec/04-computations/)). `-> {` with a space is an arrow to a record
  type.
- `none` is the empty row literal. An arrow side left out is empty too:
  `->{; E}` is `->{none; E}`, `->{R;}` is `->{R; none}` (see
  [effects.md](../effects/#empty-rows-none)). `{Users, Log}` is a closed row, a
  trailing `| r` opens it.
- `T^c`, `T^{s}`, `T^(c + d)` qualify a type with the captures it holds; `^`
  binds tighter than application, so `List a^c` is `List (a^c)`.
- Kinds use the same grammar: `Type`, `Type -> Type`, `ServiceRow`.

```flick
decode_pair :
  (forall a. Schema a -> Unknown ->{r; SchemaError} a) ->
  Unknown -> Unknown ->{r; SchemaError} (User, Settings)

open_rows : a ->{Users, Log | r; NotFound | e} b

fields : forall fields. { name : String | fields } -> { name : String }

save_with :
  SaveProps c -> (Eff {Users} {SaveError} Unit)^c
```

## Expressions

```text
block       = statement { line break statement }
statement   = "let" … | signature | expr
expr        = "fun" atom_pattern { atom_pattern } "->" block
            | "match" expr "with" arm { arm }
            | "try" block "with" arm { arm }
            | "if" expr "then" block [ "else" (if_expr | block) ]
            | operator expression over application (see the operator table)
arm         = "|" or_pattern ["when" expr] "->" block
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
            | "codec" postfix "{" field "=" block … "}" | "schema" Con { "." Con }
            | "sql" [name] "{" clause { clause } "}"
            | "cli" String "{" cli_clause { cli_clause } "}"
do_stmt     = bind { "and" bind } | "if" expr | statement
bind        = or_pattern "<-" expr
clause_list = for_clause { ";" | line break | before "if" … for_clause }
for_clause  = or_pattern "<-" expr | "if" expr | let_decl
clause      = ("from" | "join" | "left_join") name "in" expr
            | clause_word expr { "," expr }
cli_clause  = ("flag" | "arg") name { String } ":" type ["=" expr]
            | ("description" | "subcommand" | "handle") expr
```

`do { … }` sequences its statements through the Prelude's `Monad`:
`p <- e` is `flat_map (fun p -> rest) e`, a bare expression is bound to
nothing, `if c` without `then` is `if c then rest else nothing` of
`Alternative`, `let` scopes over the rest, and the last statement is an
expression of the block's monad. The pattern after `<-` must be irrefutable,
and the rest of the block is a pure function: a runtime call there is
`FLICK-E001` (see [`do`, `for` and comprehensions](../../spec/06-higher-order/#do-for-and-comprehensions)).
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
only in a signature or annotation. `if` is an expression; without `else` it is `Unit`, and so must be its `then` branch (an `else ()` gets the warning `FLICK-T028`). `ignore e` discards a non-`Unit`
result of an intermediate statement (see [Direct effectful calls](../../spec/03-direct-calls/)). `await e` runs a stored `Eff` once in
the current fiber; `task { … }` builds a cold computation (see [Values of computations and their execution](../../spec/04-computations/)). In
`provide e { … }` and `codec e { … }` the `{` ends the head expression.
In `cli "name" { … }` the parser fixes only the clause shapes; aliases on an
`arg`, duplicate names and the option types are checked by CLI elaboration
(see [CLI eDSL: flags, arguments and subcommands](../../spec/10-type-programming-and-edsl/#cli-edsl-flags-arguments-and-subcommands)). A `///` comment right above a `flag`, `arg` or
`subcommand` clause is its help text.

```flick
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
rest of the list. Or-patterns and guards follow Rust's rules (see [Variants, guards and or-patterns](../../spec/08-typescript-and-ffi/#variants-guards-and-or-patterns)): every alternative binds the same names, a name is bound at
most once per pattern or per function's parameters (`FLICK-S012`), and a
guard `when e` covers the whole arm. A `|` at the start of a line inside an
arm's pattern continues the or-pattern; after `->` it starts the next arm.

```flick
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

`flick fmt` changes only whitespace and line-end separators, so the AST is unchanged (it re-parses its
output and refuses with `FLICK-P003` otherwise). It only drops empty
rows of an arrow, which are the same type: `->{none; E}` becomes `->{; E}` and
`->{none; none}` becomes `->{}`; `->{R; none}` and `Eff none none a` stay as
written.

- indentation becomes two spaces per level while keeping every line's column
  relation (shallower, equal, deeper) to the blocks above it;
- one space between tokens, none inside `( )`, `[ ]`, around `.`, `^` and in
  `->{r; e}` and row braces `{Users, Log}`; one space inside other braces;
- unary `-` binds to its operand; `forall a b.` keeps the space after `.`;
- trailing whitespace is removed, runs of blank lines collapse to one, the file
  ends with one line break; comments stay where they are, a comment-only line
  is indented like code at its column;
- multi-line strings and block comments are copied verbatim;
- a line longer than 100 characters is split at its outermost structure: the
  body after `=` or `->` moves to its own line, an `if` chain gets a line per
  branch, then breaks go before the weakest operators (`|>` lines up with the
  start of its block), after type arrows (keeping `->{r; e}` whole), before
  application arguments, and finally one item per line inside the widest
  bracket (a single expression in parentheses stays hugged by them). Each
  continuation is two spaces deeper. A break is kept only if the file still
  has the same AST; a single token longer than the limit stays as it is;
- in a record, record type or record pattern every field that ends a line
  ends with `;`, the last one before `}` too, while a row tail `| r` takes
  none; in a list, tuple or `exposing (…)` whose closing bracket sits on a
  later line than its last item, that item ends with `,`, as prettier does. A
  record or list on one line keeps no trailing separator. The parser accepts
  these separators, including `;` after a field whose value is a block on its
  own lines, and the AST is the same with or without them.

`flick fmt --check` exits with `1` and reports `FLICK-P002` for each file that
would change; `flick fmt --stdin --stdin-filepath <file>` reads the source
from stdin and writes the result to stdout for editors.
