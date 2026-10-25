---
title: "10. Type-level programming and eDSLs"
---

**Accepted for the first version:** library SQL/UI/CLI DSLs with the chosen ML syntax `sql params { ... }`, `ui { ... }` and `cli "name" { ... }`; a PostgreSQL backend, a React adapter and Effect CLI. The type core is shared: rows, labels, finite indices, HKT and MPTC/FD. The examples below are Flick syntax, not Selda or Droplet.

## What Selda and Droplet show

[Selda](https://github.com/Kamirus/purescript-selda) builds SQL through a pure state monad and typed column expressions; [Droplet](https://github.com/easafe/purescript-droplet) is closer to SQL combinators, where the AST type contains clauses, fields and the projection. Both libraries tie the structure of a query to the type of its result through rows, multiparameter classes and functional dependencies.

| Task | Observation from the sources | Decision for Flick |
| - | - | - |
| Building a query | In Selda, `Query s a` wraps a pure `State GenState a` | Building the AST is pure; touching the database is a separate runtime call |
| Table and result fields | A row is turned into a record of SQL expressions, then into a record of values | Generic operations over rows; the user works with ordinary fields |
| LEFT JOIN | Both libraries make the right side nullable without doubling Maybe | Nullability as a separate index of the SQL expression; the output is `Option a` |
| Projection inference | In Droplet, `ToQuery q projection` has the dependency `q -> projection` | The result type is computed uniquely from the completed query |
| Hiding a large type | Droplet's internal `buildQuery` erases the AST into SQL and parameters, keeping the projection | A public opaque Query with a short signature, with no need to write the type of the whole AST |
| Handling special cases | The sources contain ordered `else instance` chains | A direct port conflicts with no-overlap; non-overlapping indices and deterministic row/type operations are needed |

The sources of Selda were checked at commit `a81f47f9054e9ea893d5ef79586be70068462249`: [Query.Type](https://github.com/Kamirus/purescript-selda/blob/a81f47f9054e9ea893d5ef79586be70068462249/src/Selda/Query/Type.purs), [FromTable and nullable JOIN](https://github.com/Kamirus/purescript-selda/blob/a81f47f9054e9ea893d5ef79586be70068462249/src/Selda/Query.purs#L197-L302). For Droplet, commit `990dd8ce078436bdb6c9ffe280dfdc97c516802b`: [ToQuery](https://github.com/easafe/purescript-droplet/blob/990dd8ce078436bdb6c9ffe280dfdc97c516802b/src/Droplet/Language/Internal/Translate.purs#L99-L145), [nullable instance chains](https://github.com/easafe/purescript-droplet/blob/990dd8ce078436bdb6c9ffe280dfdc97c516802b/src/Droplet/Language/Internal/Syntax.purs#L1507-L1516), [internal buildQuery](https://github.com/easafe/purescript-droplet/blob/990dd8ce078436bdb6c9ffe280dfdc97c516802b/src/Droplet/Language/Internal/Translate.purs#L1161-L1172).

For SQL the target set chooses restricted MPTC/FD and compiler-owned row primitives. Non-overlapping indices and a typed AST replace the cases where the original PureScript libraries use ordered instance chains. A letter-for-letter port of such instances is not promised; the original coherence rules are preserved.

## Minimal type-level capabilities

| First-version mechanism | What it expresses | Boundary |
| - | - | - |
| Type, FieldRow = Row Type, Symbol, Constraint, Region, CaptureRow | Data, fields, constraints and lifetime | Different row kinds are not mixed |
| User-defined finite kinds | Closed nullability/phase indices | Only finite nullary constructors; no general promotion/dependent values |
| Exact structural records and row polymorphism | Known fields and an open tail | No implicit width subtyping |
| HasField, Lacks, Project, Remove, UnionDisjoint | Access, projection, absence and field collisions | Compiler-owned deterministic primitives |
| Map over a row and canonical labels | Transforming every field type with a Type → Type constructor | Not a general language of type families |
| MPTC, superclasses, FD | Relations between the AST, projections and parameters | Strict coverage, non-overlap, termination |
| Opaque indexed AST | Scope, phase, nullability and a checked Query | Cannot be bypassed through a public constructor |
| Higher-rank callbacks and captures | Fresh DSL scopes and callbacks | Explicit forall; provenance is preserved |
| Diagnostic constraints | An error with a label and a source range | Not arbitrary compile-time JS |

HKT alone does not compute which fields appear after a JOIN. Higher-rank alone does not transform a record. Operations over the structure of types are what is needed.

A user-defined finite kind is declared, for example, as `kind Nullability = NonNull | Nullable`; its constructors are only type-level indices. Such declarations create neither runtime values nor a general promotion mechanism. User-defined open/closed type functions, associated types, instance chains, impredicativity and dependent types are not part of the first version. If a relation does not pass strict coverage, the library moves the computed projection into a parameter of the typed AST or uses a fixed row primitive, rather than enabling UndecidableInstances.

### Three ways to implement the type-level part

| Approach | Benefit | Cost |
| - | - | - |
| A dedicated SQL checker in the compiler | A fast path to good SQL errors | The compiler knows the domain; new DSLs need new special rules |
| Library relations on MPTC/FD and row primitives | Extensibility through ordinary libraries; the same mechanism is needed for Schema and UI | Termination/coherence and clear diagnostics are required |
| Closed functions over types | Convenient notation for transformations such as projection/defaults/nullability | A separate normalizer and reduction rules for equalities; open inputs may stay unevaluated |

The second path is chosen: shared row primitives and library relations. The SQL elaborator translates surface syntax into typed constructors and attaches source ranges; PostgreSQL semantics do not become a set of special rules of the universal checker. Closed type functions are excluded from the first version.

Normalization is cached and has an explicit budget: the initial default is 100,000 solver steps per checked definition and a depth of 256 class constraints. The build configuration can raise the limit; exceeding it produces a diagnostic with the chain of constraints. The budget does not replace termination rules and does not let unchecked code be accepted. Type-level computations do not run arbitrary JS and do not access the network or the database.

## SQL: a short signature, full inference of the result

The public API uses distinct opaque types:

- `Sql.Template dialect params result` — a plan of the query shape with named parameters that are not yet bound.

- `Sql.Query dialect result` — a query with bound parameter values.

- Internal fragments/expressions — intermediate types with scope and phase indices; they cannot be executed directly.

`Template` does not promise server-side PREPARE: that is a separate driver optimization. The query completion boundary checks all references and clauses, then hides the large AST type. This preserves our rule of a full signature above every named function and makes it possible to do without `_` in the public query type.

Suppose the tables Users and Accounts are described. `Users.id` and `Accounts.user_id` have type UserId, `Users.name` is String, and the original `Accounts.balance` is a non-null BigDecimal.

```flick
type alias BalanceRow =
  {
    name : String;
    balance : Option BigDecimal;
  }

balances :
  Sql.Template Pg { user_id : UserId } BalanceRow
let balances =
  sql params {
    from user in Users
    left_join account in Accounts
      on account.user_id == user.id
    where user.id == params.user_id
    select {
      name = user.name;
      balance = account.balance;
    }
  }

load_balances :
  UserId ->{Database; EncodeError, DbError, DecodeError} List BalanceRow
let load_balances user_id =
  let query = Sql.bind balances { user_id = user_id }
  Database.query query
```

`params` is the binder of the chosen SQL grammar; its fields are inferred from the expected Template. `Sql.bind` purely binds inert values without performing I/O. In the first version, parameter/response codecs are certified as pure; their expected failures are not lost. `Database.query` has the contract `Sql.Query Pg a ->{Database; EncodeError, DbError, DecodeError} List a`: it encodes the parameters, executes the query and checks the response. Effectful enrichment runs after the query as ordinary runtime code; serviceful SQL codecs are deferred.

The type of balance becomes `Option BigDecimal` precisely because of the LEFT JOIN. If the JOIN is replaced with an INNER JOIN, the nullable cause from that join disappears, but a column that was nullable to begin with still remains nullable. A repeated LEFT JOIN must not produce nested `Option (Option a)` merely because of two identical causes of absence.

A call to `load_balances` is an ordinary direct effectful call. Keeping it for later looks like this:

```flick
deferred_balances :
  UserId -> Eff {Database} {EncodeError, DbError, DecodeError} (List BalanceRow)
let deferred_balances user_id =
  task { load_balances user_id }
```

`sql params { ... }` builds a pure AST; without the sugar, the same library combinators are available. An ordinary let does not bind an arbitrary monad. Generic do is not part of the first version.

A `sql params { … }` block consists of clauses — one per line or separated by `;`; the continuation of a clause is written with a deeper indent. The optional name after `sql` binds the query parameters. `from x in T`, `join x in T` and `left_join x in T` introduce an alias; `on e` refines the preceding join and may stand on the next line. The remaining clauses are a word followed by comma-separated expressions: `where`, `group_by`, `having`, `order_by`, `limit`, `offset`, `select`, `union`, `union_all`, `insert_into`, `values`, `update`, `set`, `delete_from`, `returning`. Inside the block these words terminate a function application, so a clause can begin without a separator. The parser fixes only this form; types, scopes, phases and the dialect are checked by SQL elaboration.

### What is checked in SQL

| Check | Example of a rejected program |
| - | - |
| Fields and column types | `user.nmae`; comparing UserId with Date |
| Scope and aliases | A reference to an alias of a neighboring completed query; an ambiguous unqualified name |
| Projection | Two results with the same name without an explicit renaming policy |
| Nullable JOIN | An attempt to get a plain BigDecimal from a nullable balance |
| Phases and aggregation | A non-aggregated field in the SELECT of an aggregate query without a valid grouping |
| INSERT | A missing required field; a value of the wrong type; a write to a generated-always field |
| Dialect | An operation that does not exist in the chosen backend |
| Parameters | An unknown parameter name; an unsuitable codec |

The internal opaque `Sql.Expr scope phase nullability sql_type` preserves the indices; its constructors belong to the DSL. LEFT JOIN changes nullability, group_by creates a grouping witness, and an aggregate expression cannot be substituted into WHERE. Alias scopes are fresh; a completed Query does not leak local expressions. The first version supports only uncorrelated subqueries; outer correlation and LATERAL are out of its scope.

SQL uses its own three-valued logic. `==` inside a SQL block is translated into an SQL comparison, not into the language's ordinary `Eq`. WHERE keeps rows with SQL TRUE; SQL UNKNOWN is not automatically turned into a source Bool. Separate `is_null`, `is_not_null` and null-safe equality are needed. Aggregates also have their own rules: MAX over an empty set may produce NULL, COUNT has a different contract.

### Data and SQL text

Parameters and identifiers are different channels. User values are passed through placeholders and the driver's parameter array; they are not concatenated into SQL. Table, alias and column names come from metadata or a checked Identifier and are correctly encoded by the backend. A runtime String is not implicitly turned into an SQL fragment.

Raw SQL remains a separate, explicitly unsafe API with a specified result codec. It does not get the static guarantees of the typed AST. Substituting a user-supplied table name into a value position must not be called parameterization.

### The Schema and SQL boundary

The first dialect is **PostgreSQL**. Table metadata explicitly specifies SQL types, nullability, keys, generated/identity/default and the link to pure domain/wire codecs. Metadata is written in source or generated by a separate tool from migrations/the schema; ordinary compilation does not connect to the database. A domain Schema does not infer SQL constraints automatically and does not turn any refinement into a CHECK.

| SQL subset | First version |
| - | - |
| SELECT/FROM/WHERE and an explicit projection | Yes |
| INNER JOIN and LEFT JOIN | Yes |
| GROUP BY/HAVING, the main aggregates | Yes, with phase checking |
| ORDER BY, LIMIT/OFFSET | Yes |
| Uncorrelated subqueries, EXISTS/IN | Yes |
| UNION/UNION ALL | With compatible projections |
| INSERT/UPDATE/DELETE and RETURNING | Yes; required/default/generated columns are checked |
| Correlated/LATERAL, recursive CTE, window functions | After the first version |
| RIGHT/FULL JOIN, arbitrary extensions, DDL/migration DSL | After the first version |

Outside the safe subset, a separately named unsafe/raw API with an explicit result decoder is available.

SQL bigint maps to BigInt, decimal to BigDecimal; Number is allowed only through an explicitly checked range/precision. SQL NULL is an expression index and an Option on output; a source Option as a domain value does not replace this index. Schema drift and response mismatch yield DecodeError, parameter encoding yields EncodeError, and driver/server/constraint failures yield DbError with inert diagnostics. Defects and interruption keep the common runtime contract.

Cancellation does not promise to roll back SQL that has already been executed. The driver does not return the connection to the pool until a coordinated drain/close/quiescence. Types do not prove that the database is up to date, permissions, plan cost, transaction isolation or the number of rows without a separate cardinality contract.

## UI eDSL: types of props, children and actions

The base model is `Ui r e`: a pure description of an interface with deferred handlers that need services r and may finish with errors e. Building the UI does not run these handlers.

```flick
type alias SaveButtonProps c =
  {
    label : String;
    disabled : Bool;
    on_save : (Unit ->{Users; SaveError} Unit)^c;
  }

save_button :
  SaveButtonProps c -> (Ui {Users} {SaveError})^c
let save_button props =
  Ui.button
    {
      disabled = props.disabled;
      on_click = Ui.Event.exhaust (fun _ ->
        task { props.on_save () }
      );
    }
    [Ui.text props.label]
```

`on_click` has the opaque type `(Ui.Handler ClickSnapshot r e)^c`. Its builder takes a callback that creates a cold Eff and an explicitly chosen concurrency policy. In the example, Exhaust ignores new clicks until the current task finishes. The pure `save_button` does not run `props.on_save` or Ref.get during render; state arrives through the model/props. General effectful hooks inside a pure render are not allowed. Captures of props/handlers are preserved in Ui.

This agrees with [React](https://react.dev/reference/rules/components-and-hooks-must-be-pure)'s requirement of repeatable render, but the purity and lifetime checking is a Flick contract.

### UI syntax in Flick

`ui { ... }` is the chosen sugar over the same checked calls:

```flick
save_button :
  SaveButtonProps c -> (Ui {Users} {SaveError})^c
let save_button props =
  ui {
    button
      {
        disabled = props.disabled;
        on_click = Ui.Event.exhaust (fun _ ->
          task { props.on_save () }
        );
      }
      [text props.label]
  }
```

JSX `<...>` is not added in the first version; `ui { ... }` and ordinary Ui functions build one representation: the block only opens the names of the `Ui` module (a local binding wins, then the name `Ui`, then the module's names) and yields the same typed IR. This keeps the ML syntax and uniform rules for props/callbacks. [ReScript components](https://rescript-lang.org/docs/react/components-and-props/) serve as an example of a typed façade over React; Flick's surface syntax was chosen independently.

The first renderer is the **React adapter**. A custom reconciler/DOM renderer, SSR, hydration, React Server Components and the full hooks model are outside the first version. The adapter preserves React's component boundaries, key/ref and lifecycle; the optimizer does not replace a component with an arbitrary direct function call. The runtime state of the bridge lives outside the pure component body.

### Props and children

Props are an exact structural record. A missing required field, a typo or a wrong type is an error. Spread uses a disjoint row merge; overwriting only through an explicit update. Optional/defaults follow the general language, and JS undefined appears only in the adapter. Children are an ordinary argument of the chosen type. Renderer key/ref are separate metadata, not hidden fields of the user's props record.

| Children contract | Data type |
| - | - |
| Any number of nodes | `List (Ui r e)` |
| Exactly one node | `Ui r e` |
| An optional node | `Option (Ui r e)` |
| Named slots | A record with header/body/footer |
| A non-empty set of tabs | `NonEmpty (Tab r e)` |
| An element without content | The API has no children argument |

If a component must accept specifically Tab, we keep the Tab type until the container is assembled. After any child is erased into a general Ui, it is too late to find out its origin. An ordinary signature also does not prove key uniqueness in a dynamic list.

An ordinary generic component does not require general type families:

```flick
type alias SelectProps a r e c =
  {
    items : List a;
    selected : Option a;
    key : (a -> String)^c;
    render_item : (a -> (Ui r e)^c)^c;
    on_change : (Ui.Handler a r e)^c;
  }
```

`on_change` is a handler with a mandatory policy (`Ui.Event.latest …`), and the event snapshot is the selected item: a bare function `a -> Eff r e Unit` would mean an implicit fire-and-forget. All fields are tied by a single a. It is impossible to combine items of User with a handler for Product. Row operations will be needed for generic wrappers, defaults and a controlled spread; higher-rank for special polymorphic render callbacks, not for every generic component. [TypeScript JSX](https://www.typescriptlang.org/docs/handbook/jsx.html) also separates checking of intrinsic props and children; Flick's DSL can express these contracts with ordinary functions without reproducing the whole TS type system.

### Event execution and lifetime

A DOM callback that expects void does not run the returned Eff. The adapter must obtain an inert event snapshot, synchronously decide on the default action, start a managed task and observe its completion. `preventDefault` cannot be deferred to an arbitrary await; a declarative policy or a pure synchronous decision callback is suitable.

`e` in Ui describes the selected expected errors of future handlers. A UI root necessarily receives a supervisor of the full `Failure e`; infrastructure defects and shutdown interruption are distinguishable. `Ui.Dom.container id` looks up an optional `Ui.Dom.Container`. `Ui.Dom.mount container supervisor ui` renders `ui` with the registry of `Reactivity.Atom.Registry`; a view (`Ui.view`) reads atoms through `Atom.Read` while it renders and renders again only when an atom it read changes, and handlers write atoms with `Atom.set`/`Atom.update` through the service `Atom.Write`; `Atom.layer` provides the registry as both. The initial DOM mount is synchronous. React 19 may coalesce later renders; this does not promise a commit after every task or one render per animation frame. `Ui.Dom.mount` lives until the root is closed and cleanup is complete. A completed mount does not promise to return future failures through its ordinary result. A failure of the supervisor itself ends the root as a defect and is passed to the runtime reporter, without recursive reporting.

`Ui.mount_with renderer supervisor ui` has the same registry, services, errors, captures and shutdown contract, using an opaque `Ui.Renderer` factory instead of a DOM container. The factory opens the root only when the Effect runs and supplies `render(node)` and synchronous `unmount()` operations. `Ui` imports only `@flick-lang/runtime/ui/react`; DOM mounting belongs to `Ui.Dom` and `@flick-lang/runtime/ui`, so a compiled `Ui.mount_with` consumer need not load `react-dom`. HTML constructors remain DOM-specific. React Native, Ink and React Three Fiber adapters and their typed host primitives are not implemented.

Sibling keys preserve subtree and event-policy identity on reorder, while stable component-owned listeners receive the committed handler. Views over atoms with Effect handlers are Elm-like, not a complete TEA program: the serialized message loop, `init`/`update` commands and model-driven subscriptions are still missing. See the [UI reference](../../reference/ui/#prioritized-gaps) for the prioritized gaps and implementation evidence.

A policy is mandatory for every handler; there is no implicit fire-and-forget.

| Constructor | Behavior |
| - | - |
| `Ui.Event.exhaust` | Ignores new events until the current task has finished |
| `Ui.Event.latest` | Cancels the previous one, waits for stop/quiescence, then starts the last snapshot; there is a single pending slot |
| `Ui.Event.queue capacity` | A bounded FIFO; when full, a new event is rejected and the supervisor receives an infrastructure overflow defect |
| `Ui.Event.concurrent limit` | No more than limit active tasks; when saturated, a new event is rejected with an infrastructure overflow defect, with no hidden unbounded queue |

Capacity/limit are checked positive integers; constants are checked by the compiler, dynamic values by a checked constructor. Infrastructure defects are not passed off as a new element of the typed row `e`. Cancellation does not roll back an external action that has already taken place. The default action is preserved if a declarative policy or a pure synchronous decision does not require preventDefault.

Capture checking passes through props, callbacks, Eff and Ui. Returning a button whose handler captured a file from an already closed scope is an error. It is acceptable to open the file inside a task or to keep the UI within the file's lifetime region. The presence of `Ui none e` does not prove the absence of a captured scoped resource.

React cleanup synchronously closes event intake and removes registrations. The app-owned close task interrupts and joins owned fibers, waits for foreign quiescence and only then releases resources. `Ui.Dom.mount` and `Ui.mount_with` hold the app owner until this work is done. The renderer must unmount its React components, not merely hide its surface. The React cleanup callback itself is not declared a promise to wait for async finalizers. [React useEffect](https://react.dev/reference/react/useEffect) defines lifecycle cleanup; the two-phase wait is an additional contract of the Flick bridge.

### Content safety and interop

`Ui.text : forall r e. String -> Ui r e` (polymorphic over strings so that it can stand among the children of any element) inserts text through a safe text channel. URLs and raw HTML have separate policies and opaque types, for example SafeUrl and SanitizedHtml. Their constructors validate the input or are explicitly trusted; a plain String cast does not create a proof of safety. This is a separation of channels, not a promise to automatically prove the entire HTML grammar, accessibility and the correctness of the application.

A `.d.ts` can give the props shape of a foreign component, but does not prove purity, retention and cancellation. A foreign component is stored as an opaque renderer component; a pure source builder does not execute it. The adapter sets the callback/lifetime policy and the owner. The supported part of the React ecosystem that is imported goes through the same FFI pipeline; unrepresentable guarantees are not erased.

## CLI eDSL: flags, arguments and subcommands

A command-line application is described by a value `Cli.Command r e`: a pure description of flags, positional arguments, subcommands and a deferred handler that needs services r and may finish with errors e. Building a command parses and runs nothing. The runtime part is [Effect CLI](https://effect.website/docs/v4/api/effect/cli/Command); help, completion and parsing are taken from it, while option types, completeness and captures are a Flick contract.

```flick
greet : Cli.Command {Platform} none
let greet =
  cli "greet" {
    description "Печатает приветствие"
    /// Print in capital letters
    flag loud : Bool
    /// How many times to repeat
    flag times "n" : Number = 1
    /// Whom the greeting is addressed to
    arg name : String
    handle fun { loud; times; name } ->
      let text = "Hello, " ++ name
      let line = if loud then String.to_upper text else text
      Platform.print_line (String.join "\n" (List.repeat times line))
  }
```

`cli "name" { … }` is sugar over the functions `Cli.command`, `Cli.flag`, `Cli.arg`, `Cli.description`, `Cli.subcommands` and `Cli.handle`; without the sugar the same combinators are available, and both forms build one representation. The block consists of clauses — one per line or separated by `;`:

| Clause | Meaning |
| - | - |
| `description e` | The command's help text, `e : String`; at most one |
| `flag name "alias"… : T [= default]` | A named parameter `--name`; the strings after the name are additional names |
| `arg name : T [= default]` | A positional argument; the order of clauses is the order on the command line |
| `subcommand e` | A nested command, `e : Cli.Command r e` |
| `handle f` | The handler, `f : Options ->{r; e} Unit` |

The clause words are contextual: they are special only at the start of a clause inside `cli { … }`. A doc comment `///` directly above `flag`, `arg` or `subcommand` becomes its help text; the same text is shown by `flick doc`.

### Options as an exact record

Elaboration infers an exact record of options: one field per each `flag` and `arg` in declaration order, the field name being the clause name. In the example, the handler receives `{ loud : Bool; times : Number; name : String }`; a typo in the record pattern or an access to a nonexistent field is an ordinary type error. The name on the command line is obtained from the field name by replacing `_` with `-`: `dry_run` → `--dry-run`. An additional name of one character gives the short form `-n`, a longer one gives `--name`; hyphens are not written in the string.

The type of the value determines both parsing and multiplicity:

| Type in the clause | Behavior |
| - | - |
| `Bool` | A toggle flag; without `= default` it equals `false` |
| `T` | A required parameter if there is no `= default` |
| `Option T` | Optional; `= default` is forbidden |
| `List T` | A repeatable flag (0 or more); for `arg`, a variable-length tail, only in the last position |
| `NonEmpty T` | Like `List T`, but at least one value |

`T` needs an instance of the class `Cli.Value T`, which parses a string into a typed error. The library provides instances for `String`, `Number` (rejects NaN and the empty string), `BigInt`, `Path`, `Duration` and `Redacted String`; a `Redacted` value is not printed in help, errors and logs. `deriving Cli.Value` builds an instance for an ADT of nullary constructors — a choice among the constructor names in kebab-case — and for a refined type if the base type has an instance: parsing calls `T.parse`. A default is a pure expression of type `T`; in help it is shown through `Show T` if such an instance exists.

### Subcommands and the handler

```flick
tool : Cli.Command {Platform} none
let tool =
  cli "tool" {
    description "Утилиты проекта"
    /// Verbose output
    flag verbose "v" : Bool
    subcommand greet
    subcommand version
  }
```

The services and errors of a command are the union of the rows of its handler and subcommands, as for any compound expression. A subcommand's handler receives only its own record of options; the parent's flags are parsed before the subcommand name and are visible only to the parent's handler. Configuration common to all subcommands is passed as a service provided in `main`. A command without `handle` must have subcommands; invoking it without a subcommand prints help and ends with `CliError`. A command with subcommands does not declare an `arg`, otherwise a positional argument is indistinguishable from a subcommand name.

Compile errors: a repeated field, flag or subcommand name, a collision of a long or short name, including with the reserved `--help`, `-h`, `--version` and `--completions`; `= default` on `Option`; `List`/`NonEmpty` on an `arg` that is not last; a type without `Cli.Value`; a default of the wrong type; `handle` or `description` twice; a command with neither `handle` nor subcommands; an `arg` next to a `subcommand`.

### Running

`Cli.run : Cli.Command r e -> List String ->{Platform | r; CliError | e} Unit` parses the arguments with a pure function and only after successful parsing executes the handler in the current fiber. `--help`, `--version` and `--completions bash|zsh|fish` print the result to stdout and finish without calling the handler. Invalid input — an unknown flag, a missing required argument, a value that `Cli.Value` could not parse — yields `type error CliError = CliError String` with text for the user; the handler is not run.

```flick
main : Unit ->{; AppError} Unit
let main () =
  provide Platform.live {
    try
      Cli.run tool (Platform.args ())
    with
    | CliError message -> App.fail message
  }
```

A command remains an ordinary value: the entry point is still the single `main : Unit ->{; AppError} Unit`, and exit codes and the order of cleanup follow the general rules of section 12. Cancellation by SIGINT interrupts the handler, and its resources are closed under the owner contract of section 7. Captures pass through the handler just as through UI callbacks: a command whose handler captured a resource of a closed scope cannot be returned from that scope. Interactive prompts (`Cli.Prompt`) are ordinary effectful functions inside the handler, not clauses of the block.

## What SQL and UI add to design validation

The accepted design is validated by two mandatory end-to-end prototypes; their result may reveal a defect in the specification, but the choice of mechanisms has already been made:

- SQL: typed table → LEFT JOIN → nullable projection → bind parameters → Effect execution; negative examples with an unknown field, an incorrect grouping, a leaked alias and a wrong parameter.

- UI: generic props → typed children → pure render → task callback → managed completion; negative examples with a wrong prop, a runtime call in render and a capture of a closed resource.

They validate expressiveness and the cost of inference. For large rows and nested queries, checking time and diagnostic quality are measured; successful small examples do not prove scalability. Optimizations may precompile the static shape of SQL and the pure creation of UI, preserving parameters, capture metadata, renderer boundaries and the observable execution of Effect.
