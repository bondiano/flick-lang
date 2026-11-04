---
title: "Type-level programming and eDSLs"
---

Flick's SQL, UI and CLI DSLs — `sql params { … }`, `ui { … }` with `component` declarations, and `cli "name" { … }` — are libraries over one type core: rows, labels, finite kinds, HKT and multiparameter classes with functional dependencies. Their backends are PostgreSQL, React (in the browser and on the server) and Effect CLI.

## Type-level capabilities

| Mechanism | What it expresses | Boundary |
| - | - | - |
| Kinds `Type`, `Row k`, `Symbol`, `Constraint`, `Region`, `CaptureRow`, `ServiceRow`, `ErrorRow` | Data, fields, labels, constraints and lifetime | Different row kinds are not mixed |
| User-defined finite kinds (`kind`) | Closed indices such as nullability or phase | Only nullary constructors; no general promotion of values |
| Exact structural records and row polymorphism | Known fields and an open tail | No implicit width subtyping |
| Row primitives `HasField`, `Lacks`, `Remove`, `Project`, `UnionDisjoint`, `Map`, `Labels` | Access, absence, removal, projection, disjoint merge, mapping every field type, canonical labels | Solved by the compiler from known records; a program cannot declare instances of them |
| Multiparameter classes, superclasses, functional dependencies | Relations between an AST, its projections and its parameters | Coverage, non-overlap and a solver budget |
| Opaque indexed types | Scope, phase, nullability and a checked query | No public constructor to bypass them |
| Higher-rank callbacks and captures | Fresh DSL scopes and callbacks | Explicit `forall`; captures are preserved |

The row primitives are described in [Row primitives](../../reference/classes/#row-primitives). HKT alone does not compute which fields appear after a JOIN, and higher-rank types alone do not transform a record: operations over the structure of types are what the DSLs need.

A finite kind is declared, for example, as `kind Nullability = NonNull | Nullable`; its constructors are only type-level indices and create no runtime values. Flick has no type families, associated types, instance chains, impredicativity or dependent types. If a relation does not pass the coverage rules, a library moves the computed type into a parameter of its typed AST or uses a row primitive.

Constraint solving has a budget per checked definition: `solver-steps` (100 000) and `solver-depth` (256 nested constraints) in `[build]` of `flick.toml`. Exceeding it is `FLICK-C009`, with the chain of constraints; running out is never a success. Type-level computations do not run JS and do not access the network or the database.

## SQL: a short signature, full inference of the result

`sql table Name "table" { … }` declares the row type `Name` and its table; the compiler never connects to a database. `sql [p] { … }` is a pure, checked `Sql.Template Pg params row`. The name after `sql` binds the parameters, whose types follow from where they are used; `$name` captures a value of the program as a placeholder.

```flick
type alias BalanceRow = {
  name : String;
  balance : Option BigDecimal;
}

balances : Sql.Template Pg { user_id : UserId } BalanceRow
let balances = sql params {
  from user in Users
  left_join account in Accounts on account.user_id == user.id
  where user.id == params.user_id
  select {
    name = user.name;
    balance = account.balance;
  }
}

load_balances : UserId ->{Database; EncodeError, DbError, DecodeError} List BalanceRow
let load_balances user_id =
  let query = Sql.bind balances { user_id = user_id }
  Database.query query
```

A block consists of clauses, one per line or separated by `;`; the continuation of a clause is indented deeper. `from x in T`, `join x in T` and `left_join x in T` introduce an alias; `on e` refines the preceding join. The other clauses are a word followed by comma-separated expressions: `where`, `group_by`, `having`, `order_by`, `limit`, `offset`, `select`, `union`, `union_all`, `insert_into`, `values`, `update`, `set`, `delete_from`, `returning`. Inside the block these words end a function application, so a clause can begin without a separator. The parser fixes only this form; the SQL elaboration of the checker checks the rest against the tables:

| Check | Code |
| - | - |
| Unknown column, unknown or out-of-scope alias, ambiguous column | `FLICK-Q001`, `FLICK-Q002`, `FLICK-Q003` |
| Two results with the same name | `FLICK-Q004` |
| A column neither grouped nor aggregated; an aggregate where none is allowed | `FLICK-Q005`, `FLICK-Q006` |
| An INSERT missing a required column; a write to a generated column | `FLICK-Q007`, `FLICK-Q008` |
| An unknown parameter; a construct outside the supported subset | `FLICK-Q009`, `FLICK-Q010` |
| Comparing values of different Flick types | A type error, even when both are numbers in SQL |

A result that may be `NULL` — a nullable column, a column of a `left_join`ed table, an aggregate other than `count` — has type `Option t`, once: two left joins do not make `Option (Option t)`. `==` is SQL's comparison, and `where` keeps only rows where the condition is true; `is_null`, `is_not_null` and `is_not_distinct_from` are separate functions.

`Sql.bind` binds parameters without I/O; only `Database.query` and `Database.execute` talk to the database. Values never enter the SQL text: each parameter and literal is a placeholder, and tables, aliases and columns come from metadata. A refused parameter is `EncodeError`, a response that does not match the query `DecodeError`, a refusal of the server or driver `DbError`. Correlated and `LATERAL` subqueries, recursive CTEs, window functions, `RIGHT` and `FULL` joins and DDL are outside the checked subset; `Sql.unsafe_query` runs raw SQL with an explicit decoder, and only an `Sql.Identifier` names a table or a column there. See [SQL](../../reference/sql/) for tables, transactions, migrations and cancellation.

## UI eDSL: types of props, children and actions

`Ui` is a pure description of an interface with deferred handlers. A handler's task writes atoms through `Atom.Write` and nothing else; it provides any other service itself and handles its own errors, so `Ui` carries no service or error rows. Building a `Ui` runs no handler, reads no state and touches no DOM.

```flick
type alias SaveButtonProps c = {
  label : String;
  disabled : Bool;
  on_save : (Unit ->{Atom.Write;} Unit)^c;
}

save_button : SaveButtonProps c -> Ui^c
let save_button props = ui {
  button
    {
      disabled = props.disabled;
      on_click = Ui.Event.exhaust (fun _ -> task { props.on_save () });
    }
    [text props.label]
}
```

`ui { … }` is a block in which the names of `Ui` need no `Ui.`: a local binding wins, then a name of `Ui`, then the module's own names. It compiles to the same IR as the calls `Ui.button … [Ui.text …]`, and applying a `Ui.Component p` to props inside it is `Ui.component value props`, including `props |> value`. JSX is not part of the language.

Props are exact records: a missing field, an unknown one or a wrong type is an error. Children are an ordinary argument of the chosen type — `List Ui`, `Ui`, `Option Ui`, named places, `NonEmpty Tab` — and a key is metadata given with `Ui.keyed`, never a prop. Handlers keep their captures: a button whose handler holds a file of a scope cannot leave that scope (`FLICK-G001`). `Ui.text` inserts text through a safe channel; a URL is a `SafeUrl` and raw HTML a `SanitizedHtml`, which a plain `String` never is.

### Atoms, views and components

State lives in atoms: `Atom.make value` is a state, `Atom.derived` an atom computed from the atoms it reads, and `Atom.set`/`Atom.update` write through the service `Atom.Write`. A view (a `component`, of type `Ui.View p`) is a component whose render reads atoms through `Atom.Read` and renders again only when an atom it read changes. A render is a Sync stage: a write, another service or an error in it is `FLICK-U004`. Nothing is made while rendering: an atom, an action or a view made in a render is `FLICK-U006`.

`component name (props : P) = body` declares a view of type `Ui.View P`, which `Ui.render name props` places. The body is a block: its last expression is the render, run again whenever an atom it read or its props change; the statements before it are the setup of each instance, run once when the instance mounts with the service `Setup s` of the instance's region. `Atom.local` makes an atom of the instance there, and the `Action` makers its actions.

```flick
component counter (start : Float) =
  let count = Atom.local start
  let bump = Action.exhaust (fun _ -> task { Atom.update count (fun n -> n + 1) })
  ui {
    let n = Atom.value count
    button { on_click = bump; disabled = false } [text (show n)]
  }
```

When an instance unmounts, the events of the actions its setup made stop and their tasks are interrupted. An action (`Action.exhaust`, `latest`, `queue`, `concurrent`) is a handler declared once and given to as many elements as need it; they share its policy state in a root, and `Action.with action arg` gives each element its own argument and state. See [Atoms and views](../../reference/ui/#atoms-and-views), [Components](../../reference/ui/#components) and [Actions](../../reference/ui/#actions).

### Event execution and lifetime

A handler field has the opaque type `Ui.Handler snapshot`, which only a policy makes, so no handler is fire-and-forget:

| Constructor | An event that comes while earlier tasks run |
| - | - |
| `Ui.Event.exhaust` | Is ignored |
| `Ui.Event.latest` | Interrupts the running task, waits until it stopped, then runs; at most one event waits, the newest |
| `Ui.Event.queue capacity` | Waits, `capacity` events at most; beyond that it is rejected |
| `Ui.Event.concurrent limit` | Runs, `limit` tasks at most; beyond that it is rejected |

`capacity` and `limit` are positive integers: a constant that is not one is `FLICK-U001`, a computed one is checked when the handler is made. A rejected event is an infrastructure defect for the supervisor, never an error of the typed row. The handler gets an inert snapshot taken inside the DOM callback; `Ui.Event.prevent_default decide h` decides the default action synchronously with the pure `decide`.

`Ui.Dom.mount container supervisor ui` renders `ui` with the provided atom registry (`Atom.layer`); the initial DOM mount is synchronous, later renders are React 19's to schedule. The supervisor receives `Failure none`: task failures and rejected events. Closing has two phases: React cleanup stops taking events and unmounts the tree, then the handler tasks are interrupted and joined, and only then does `mount` end, so the scope releases its resources after the tasks have stopped. `Ui.mount_with renderer supervisor ui` has the same contract with an opaque `Ui.Renderer` instead of a DOM container.

`Ui.Server.render ui` renders the markup of an initial view on the server, its views reading atoms from a registry of the request; handlers do not run there. A container marked `data-flick-ssr` is hydrated by `Ui.Dom.mount` instead of being cleared. See [Server-rendered initial views](../../reference/ui/#server-rendered-initial-views).

## CLI eDSL: flags, arguments and subcommands

A command-line application is a value `Cli.Command r e`: a pure description of flags, positional arguments, subcommands and a handler that needs services `r` and may fail with errors `e`. Building a command parses and runs nothing.

```flick
import Cli exposing (plugin cli)

greet : Cli.Command none none
let greet = cli "greet" {
  description "Prints a greeting"
  /// Print in capitals
  flag loud : Bool
  /// How many times
  flag times "n" : Float = 1
  /// Who to greet
  arg name : String
  handle fun { loud; times; name } ->
    let text = "Hello, " ++ name
    let line = if loud then String.to_upper text else text
    Platform.stdout_line (String.join "\n" (List.repeat times line))
}
```

`cli "name" { … }` is the block of the plugin of `std` (see [Plugins](../../reference/plugins/)), which a file imports. It is sugar over the combinators `Cli.command`, `Cli.flag`, `Cli.arg`, `Cli.description`, `Cli.subcommand` and `Cli.handled`, the calls it stands for. The block consists of clauses, one per line or separated by `;`; the clause words are special only at the start of a clause:

| Clause | Meaning |
| - | - |
| `description e` | The command's help text, `e : String`; at most one |
| `flag name "alias"… : T [= default]` | A named option `--name`; the strings after the name are more names |
| `arg name : T [= default]` | The next positional argument |
| `subcommand e` | A nested command, `e : Cli.Command r e` |
| `handle f` | The handler, `f : Options ->{r; e} Unit`; at most one |

A `///` comment right above a `flag`, `arg` or `subcommand` is its help text. The handler gets an exact record with a field per `flag` and `arg`, so a typo in its pattern is an ordinary type error. The type decides parsing and multiplicity: `Bool` is a switch, `T` is required unless it has a default, `Option T` is optional and takes no default, `List T` and `NonEmpty T` repeat (for an `arg`, only the last). A value is parsed by the class `Cli.Value T`. The services and errors of a command are the union of the rows of its handler and subcommands; a command keeps the captures of its handler (`FLICK-G001`).

`Cli.run command args` parses the arguments with Effect's CLI and only then runs the handler in the current fiber. `--help`, `--version` and `--completions` run no handler; invalid input fails with `Cli.CliError` and runs no handler. Duplicate names, a default on `Option`, a many-valued `arg` before the last, a repeated `description` or `handle`, a command with neither a handler nor subcommands, and an `arg` beside subcommands are compile errors of the plugin (`std/L001`–`std/L007`). See [Command-line applications](../../reference/cli-dsl/).

## `do` and library combinators

Each DSL block is sugar over ordinary library functions, which programs may call directly. Generic monadic code uses `do { … }` over the `Monad` of the Prelude, with `and` for independent bindings through `Applicative` (see [`do`, `for` and comprehensions](../04-higher-order/#do-for-and-comprehensions)); an ordinary `let` does not bind through a monad.
