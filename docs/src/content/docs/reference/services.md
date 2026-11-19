---
title: "Services, layers and the host"
---

A service is a set of operations a program looks up where it runs, a layer
builds services, and `provide` gives them to a block (see [Errors, services, resources and concurrency](../../spec/05-errors-and-concurrency/)).
How the rows of a call are checked is in [effects](../effects/).

## Services

A `service` lists operations with full signatures. Running an operation looks
the service up, so every operation has a runtime arrow (`FLICK-E006`), and
`S.op` needs `S` at each of them: with `get : UserId ->{; NotFound} User`
declared in `Users`, `Users.get` is `UserId ->{Users; NotFound} User`.
Operations are not class methods: a class method such as `show` needs no
service.

`S.Impl` is the record of the operations as declared. `Layer.succeed S impl`
makes a layer from it; the service is named as a value:

```flick
# js_now : Unit ->{} Float
# let js_now () = 0.0
#
service Clock =
  now : Unit ->{} Float

clock_impl : Clock.Impl
let clock_impl =
  { now = js_now }

clock_layer : Layer none none {Clock}
let clock_layer =
  Layer.succeed Clock clock_impl
```

A `type alias` of a service or of an error stands for it in a row:
`type alias Time = Clock` makes `->{Time}` the same row as `->{Clock}`.
An alias of a whole row is a [set](#sets-of-services-and-errors). An alias of
anything else in a row is an error (`FLICK-T002`). An alias
names a type, not a namespace: as an alias of a data type does not open its
constructors, an alias of a service does not open its operations or
`S.Impl`. A [re-export](../modules/) does: `Atom.Registry`, which `Atom`
re-exports from `Reactivity.Atom`, is that service, so `Atom.Registry.get`,
`Atom.Registry.Impl` and `Layer.succeed Atom.Registry impl` all work.

## Sets of services and errors

A `type alias` of a row names a set of services and errors. `...` merges a set
into a row, a plain name adds one element, and `-` takes one out:

```flick
# type UserId = UserId String
# type alias User = { name : String }
# type error UserNotFound = UserNotFound
# type error InvalidEmail = InvalidEmail
# type error OrderNotFound = OrderNotFound
# type error Boom = Boom
# service Log =
#   info : String ->{} Unit
# service Users =
#   get : UserId ->{; UserNotFound} User
# service Billing =
#   charge : Float ->{} Unit
#
type alias UserErrors = {UserNotFound, InvalidEmail}

type alias Reads = {Log, Users; ...UserErrors}

type alias Orders = {...Reads, Billing; OrderNotFound}

type alias Quiet = {...Orders, -Log; -InvalidEmail}

load : UserId ->{...Orders} User
let load id = Users.get id

wider : UserId ->{...Orders, Async | r; Boom} User
let wider id = load id
```

A set expands where it is named: `->{...Reads}` is the same row as
`->{Log, Users; UserNotFound, InvalidEmail}`. That gives composition without
any rule of its own:

- Sets may name each other in any order, and an element two sets hold counts
  once. A set cannot hold itself (`FLICK-R007`).
- What needs less fits where more is allowed, as for
  [any row](../effects/#rows-a-body-needs): a function of `{...Reads}` passes where one of
  `{...Orders}` is expected. Under another constructor rows stay equal, so
  `List (Eff {...Reads} none Unit)` is not a
  `List (Eff {...Orders} none Unit)` (`FLICK-T003`).
- An arm of a `try` catches one error of a set, and the rest stay in the row.

In braces a set takes `...` and one element does not, both `FLICK-T002` with a
fix. Outside braces an alias is the whole row: `Layer none none Services`,
`Eff Services Errs a`, `Services + r`. A row that merges or takes out a set may
leave out its `;`, since a set can hold errors of its own.

A set of errors alone stands on either side of `;`; a set with services only
before it (`FLICK-T002`). An arrow and `Eff` take the errors of a set into their
row of errors. Any other type with a row, such as `Layer {...Orders} none {Log}`,
has no place for them (`FLICK-T002`): name the services in an alias of their own.

`-` takes from everything the braces merge, wherever it stands, and services and
errors each go on their own side of `;`. `-...Reads` takes a whole set. All of
these are `FLICK-T002`, so a typo or a rename does not pass silently:

- taking out what the row does not hold;
- taking an element with arguments out of a row variable, as in
  `{-Store Int | r}`: the variable may hold `Store` at another type, so take
  out every instance, `{-Store | r}`, or write `{Store Int | r}` and leave `r`;
- writing an element and taking it out in one row, as in `{Log, ...Reads, -Log}`;
- `...r`, since a row variable goes after `|`; merging an alias with a tail, since
  `| r` belongs where the set is used; `opaque type alias` of a row.

Over a row variable `-` says what a function gives on its own. `{-Log | r}` is
`r` without `Log`, whether or not `r` holds it, so the caller owes everything of
its row but `Log`:

```flick
# service Log =
#   info : String ->{} Unit
# service Clock =
#   now : Unit ->{} Float
#
quiet : Layer none none {Log} -> Eff r none a ->{-Log | r} a
let quiet layer work = provide layer { await work }

timed : Layer none none {Log} -> Eff {Clock, Log} none a ->{Clock} a
let timed layer work = quiet layer work
```

Without `provide`, the body of `quiet` would need `Log` whenever `r` holds it,
which the signature does not allow (`FLICK-E002`). Errors work the same after
`;`, as in `->{; -NotFound | e}` over a `try` that handles `NotFound`.
Taking a name out is the one way a signature keeps a service or an error out
of a row variable; another constraint on open rows waits for a program this
cannot express ([E15, #438](https://github.com/bondiano/flick-lang/issues/438)).

A handler finds a service by its name, not by its arguments. `provide` of a
layer of `Store Int` answers every `Store` operation in its body, so the body
may need a row variable only where the variable holds no other instance of
`Store`: one taken out, `Eff {-Store | r} …`, or one beside the instance it
names, `Eff {Store Int | r} …`, which holds `Store` only as `Store Int`. A
variable that may hold `Store String` would get the layer's `Store Int` for it,
and is `FLICK-E019`.

A set may take parameters, as in `type alias Db s = {Scope s, Log}`.

A set widens a signature: its callers must provide all of it, and tests must too.
Use sets at the boundaries of modules, where many functions need the same things.
Leave a small function its exact row, or let the row be inferred.

## Layers and `provide`

`Layer inputs errors outputs` builds the services `outputs` from `inputs` and
may fail with `errors`. `provide layer { body }` builds the layer before the
body runs: the body may need the services the layer builds, and the block
needs the layer's inputs and errors besides the rest of what the body needs.

```flick
# type UserId = UserId String
# type alias User = { name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# service Log =
#   info : String ->{} Unit
# service Config =
#   url : Unit ->{} String
# type error ConnectError = ConnectError
# users_live : Layer {Config} {ConnectError} {Users}
# let users_live = Layer.effect Users (task { Fail.fail ConnectError })
# load_name : UserId ->{Users, Log; NotFound} String
# let load_name id = (Users.get id).name
#
application : UserId ->{Config, Log; NotFound, ConnectError} String
let application id =
  provide users_live {
    load_name id
  }
```

With `users_live : Layer {Config} {ConnectError} {Users}` and `load_name`
needing `{Users, Log}` and `{NotFound}`, the block needs `{Config, Log}` and
`{NotFound, ConnectError}`.

`Layer.merge a b` builds the services of both layers; two layers that build
the same service are `FLICK-E004`. `Layer.override a b` merges them too and
lets `b` replace what `a` builds.

The services a `provide` builds end with it. Work an owner keeps until its
scope closes (a child of `Fiber.spawn`, `FiberSet.run`, a finalizer of
`Scope.add_finalizer`, a server's handler) runs with the services of the
place that started it, so each of them must come from a provider that
outlives that owner: a `provide` around the owner's `scope`, or the caller
of a function whose body opens that `scope`. Otherwise it is `FLICK-G003`:

```flick fail G003
# service Greeter =
#   greet : String ->{} String
# report : Failure e ->{} Unit
# let report _ = ()
# greeter : Layer none none {Greeter}
# let greeter = Layer.succeed Greeter { greet = fun name -> name }
#
leaked : Unit ->{} Unit
let leaked () = scope outer {
  provide greeter {
    // FLICK-G003: the child of `outer` would use `Greeter` after the
    // `provide` released it
    ignore (Fiber.spawn outer report (task { Greeter.greet "child" }))
  }
}
```

A function whose owner and services both come from its caller says that
the services outlive the owner with `Fiber.Provided`; each call checks it:

```flick
# service Greeter =
#   greet : String ->{} String
# report : Failure e ->{} Unit
# let report _ = ()
#
start : forall s. Fiber.Provided {Greeter} s => Fiber.Owner s ->{Greeter} Unit
let start owner = ignore (Fiber.spawn owner report (task { Greeter.greet "child" }))
```

The build of `Layer.from_build f` and `Layer.from_build_memo f` runs in the
owner of the region `s` the layer is built into, and `f` assumes
`Fiber.Provided ri s` for the layer's inputs `ri`: a finalizer it registers may
use them. A `provide` and `Layer.launch` build a layer into a region of their
own, which every provider of its inputs outlives. `Layer.build`,
`Layer.build_with_memo_map`, `LayerRef.make`, `LayerMap.make` and
`LayerMap.from_record` build it into a region of their caller, so they require
`Fiber.Provided ri s` there. A build into a `scope` inside the `provide` of an
input checks; a build into a `scope` around that `provide`, which outlives it,
is `FLICK-G003`:

```flick fail G003
# service Greeter =
#   greet : String ->{} String
# service Box =
#   value : Unit ->{} String
# greeter : Layer none none {Greeter}
# let greeter = Layer.succeed Greeter { greet = fun name -> name }
#
box : Layer {Greeter} none {Box}
let box =
  Layer.from_build (fun memo ->
    Scope.add_finalizer (task { ignore (Greeter.greet "released") })
    Layer.build_with_memo_map memo (Layer.succeed Box { value = fun () -> "box" })
  )

built : Unit ->{} Unit
let built () = provide greeter { scope { ignore (Layer.build box) } }

leaked : Unit ->{} Unit
let leaked () = scope {
  // FLICK-G003: the finalizer of `box` would run when the `scope` closes,
  // after the `provide` released `Greeter`
  provide greeter { ignore (Layer.build box) }
}
```

## Explicit host dependencies

Code that touches the host names the `Host` service it uses in its row.
`Host` declares six ordinary services: `Host.Console` (standard streams),
`Host.Clock` (wall and monotonic time reads), `Host.Random` (a draw in `[0, 1)`),
`Host.Environment` (environment variables), `Host.Arguments` (the program's
arguments) and `Host.Process` (starting child processes). `Host.Live` is the
set of all six and `Host.BrowserLive` that of the first three. None has an
implementation inside a program. The layers that provide them, `Host.console`,
`Host.clock`, `Host.random`, `Host.environment`, `Host.arguments` and
`Host.process`, with `Host.live` for all six and `Host.browser_live` for the
first three, are trusted adapters over the host's APIs. They are chosen at an
entry: the launcher or the runner of `main`, a test runner, or a `provide`.

```flick
mode : Unit ->{Host.Environment} Option String
let mode () = Host.Environment.env "MODE"

selected : Unit ->{} Option String
let selected () =
  let environment =
    Layer.succeed Host.Environment { env = fun _ -> Some "test"; vars = fun () -> [] }
  provide environment { mode () }
```

Service implementations are records, so tests can capture output, fix time and
draws, or supply arguments without changing process globals. Tasks read their
services where they run; an inner provider restores the outer one when it ends.

The standard library reaches the host through these services, so the rows of
its functions name what they use:

- `Platform.stdout_line`, `stderr_line` and `stdout_write`, the five levels of
  `Console` and the built-in flags of `Cli.run` use `Host.Console`;
- `Platform.args` uses `Host.Arguments`;
- `Platform.env`, and `Config.get`, whose default provider reads the
  environment, use `Host.Environment`;
- `Clock`, `DateTime.now`, `is_past` and `is_future`, and `Jwt` use
  `Host.Clock`;
- `Random` uses `Host.Random`, so a fake one fixes every draw;
- `ChildProcess.spawn`, `string`, `lines` and `exit_code` use `Host.Process`.

`Log.console` is a layer made of `Host.console` and needs nothing: providing
it chooses the live console, as an entry does.

### Who provides them

Each place a program runs provides some of the `Host` services at its root:

| Where | `Host` services |
| - | - |
| Node | all six: `Runner.Node` provides `Host.Live`, and the launcher of an entry without a runner the ones `main` names |
| Go (`emit = "go"`) | all six, whatever `main` names |
| `Runner.Browser`, `Runner.Cloudflare` | `Host.BrowserLive`: `Console`, `Clock` and `Random`; Cloudflare provides its `Env` beside them |
| the [playground](../../play/) | the ones `main` names, with no environment variables and no arguments; a child process cannot start there |
| `flick test` | `Host.Live`, to `Test.effect_test`, `clock_test`, `suite` and `it` |
| the reference model | `Host.live` on a fixed host: no environment variables or arguments, a wall clock stopped at 1 700 000 000 000 ms, every draw 0.5 and no processes |

A runner's `Main` says which services `main` may need (see
[entry points](../project/#entry-points)), and a layer `main` provides itself
wins over the root. UI tasks (`Ui.mounted`, `on_mount`, `watched`, `watch`, the
handlers of `Ui.Event` and `Action`) and `Runner.Cloudflare.wait_until` may
use `Host.BrowserLive`. `Ui.Dom.mount` and `Ui.mount_with`, which run the UI
tasks with their caller's services, need it, so a `main` that mounts a root
names it.

### Where a missing service is caught

A body that uses a `Host` service its signature does not name is
`FLICK-E002`, as for any service. Its safe fix adds the service where the row
is written: an editor offers it, and `flick check --fix --only E002` applies
it across a program. A pure arrow, or a callback whose row another module
closes, has to be edited by hand.

At an entry, a `main` that needs a service no runner provides is
`FLICK-P026`: a service outside `Host`, or one its runner's `Main` does not
name, such as `Host.Environment` under `Runner.Browser`. `FLICK-P026` needs
an entry: a support profile of each target, which would report in a library
and in the editor what an environment cannot provide, is follow-up work ([T2, #445](https://github.com/bondiano/flick-lang/issues/445)).

### What an empty row does not promise

A row names the host access that goes through the `Host` services. It is not a
sandbox against code you did not write, since some code reaches the host
whatever its row says:

- a `foreign` binding is taken on trust: `now : Unit ->{} Float` bound to
  `Date.now` reads the clock with an empty row;
- `Host.raw_*` and the live layers made of them are ordinary values: any
  function may call the first or provide the second, whatever its own row:

```flick
printed_anyway : Unit ->{} Unit
let printed_anyway () =
  provide Host.console { Platform.stdout_line "no row names this" }
```

Review foreign bindings and the uses of the `Host` layers as you review any
trusted code.

### Reading the time and waiting

`Host.Clock` reads the time; it does not make anything wait. `Effect.sleep`
(`Duration ->{Async} Unit`), `delay` and the schedules of `Effect.retry` wait
on the scheduler and need no `Host.Clock`. The live `Host.Clock` reads the
clock of the fiber, the one the scheduler waits on. `Test.clock_test` replaces
that clock with virtual time, which starts at 0 and moves only by
`Test.adjust` and `Test.set_time`, which need its `VirtualTime`: a sleep waits
for virtual time, and `Clock.current_time_millis` and `DateTime.now` read it.
A `Host.Clock` you provide yourself changes only what the reads return; timers
still follow the fiber's clock.

## Not checked yet

| Construct | Today |
| - | - |
| `Layer.merge` used other than applied to both layers | conflicts are not reported |
