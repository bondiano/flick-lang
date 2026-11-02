# A task planner built from monad transformers

How do you combine an optional result, changing state and shared configuration
without manually unpacking all three at every step? A transformer adds one of
those behaviors to an existing computation. This example implements the wrappers
and their instances in Flick, then uses them in a small command-line planner.

The progression follows the idea in the [Monday Morning Haskell transformer
tutorial](https://mmhaskell.com/monads/transformers): start with the representation,
define how actions compose, then stack the wrappers. Haskell's
[`mtl`](https://github.com/haskell/mtl) defines capability classes on top of
transformers. This example covers an enumerated set: five transformers, shared
`MonadTrans`, and state, reader, writer and error capability classes with
functional dependencies. It is not every transformer in Haskell's ecosystem.

## Run it from the repository root

Use the checkout's [source setup](../../README.md#try-it-from-source), including
the pinned Rust toolchain, Node.js and pnpm:

```sh
pnpm install --frozen-lockfile
cargo build -p flick

./target/debug/flick run --project examples/mtl -- catalog
./target/debug/flick run --project examples/mtl -- plan --budget 60 docs tests lint
./target/debug/flick run --project examples/mtl -- plan --budget 36 --overhead 3 docs lint
./target/debug/flick run --project examples/mtl -- plan --budget 25 docs lint
./target/debug/flick run --project examples/mtl -- --help
./target/debug/flick run --project examples/mtl -- plan --help
./target/debug/flick test --project examples/mtl
./target/debug/flick run --project examples/mtl --entry App.Capabilities
```

If `flick` is already on your `PATH`, replace `./target/debug/flick` with `flick`.
The 25-minute command intentionally fails; run subsequent commands separately
rather than joining the whole block with `&&`.

The catalog is `docs: 20`, `tests: 30`, `lint: 10` minutes. Plans process task names
in order, charging overhead for **each** accepted task, including repetitions.

| Plan | Completed tasks | Remaining | Outcome |
| --- | --- | --- | --- |
| 60 minutes: `docs tests lint` | `docs`, `tests`, `lint` | 0 | Success: an exact fit is allowed |
| 36 minutes, overhead 3: `docs lint` | `docs`, `lint` | 0 | Success: `(20 + 3) + (10 + 3)` |
| 25 minutes: `docs lint` | `docs` | 5 | Stops at `lint`, prints the prefix and exits nonzero |

An unknown task stops the plan in the same way as an over-budget task. Neither
consumes time, and no subsequent task runs. The CLI requires a nonnegative safe
whole-number budget and accepts an optional overhead with the same validation;
omitted overhead is zero.

### What the commands print

`catalog` lists the base cost, without the configurable overhead:

```text
Available tasks (before per-task overhead):
  docs: 20 min
  tests: 30 min
  lint: 10 min
```

The successful 60-minute plan prints:

```text
Budget: 60 min; overhead: 0 min/task
Completed: docs, tests, lint
Remaining: 0 min
Plan complete.
```

With `plan --budget 36 --overhead 3 docs lint`, both tasks still fit:

```text
Budget: 36 min; overhead: 3 min/task
Completed: docs, lint
Remaining: 0 min
Plan complete.
```

With `plan --budget 25 docs lint`, the partial result is printed before failure:

```text
Budget: 25 min; overhead: 0 min/task
Completed: docs
Remaining: 5 min
```

The error reports `Unscheduled: lint`, explains that the first unscheduled task
is unknown or does not fit including overhead, and exits with code 1. It does
not silently skip that task and try later ones. For example,
`plan --budget 60 docs missing lint` retains only `docs`, leaves 40 minutes,
and reports `Unscheduled: missing, lint`. An empty completed prefix is printed
as `Completed: (none)`.

### A typed CLI, not a hand-written argument parser

[`App.Main`](src/App/Main.fl) describes `catalog` and `plan` with Flick's
[`cli` block](../../docs/src/content/docs/reference/cli-dsl.md). The handler
receives a typed record:

| Declaration | Command-line behavior |
| --- | --- |
| `flag budget "b" : Minutes` | Required `--budget` / `-b` |
| `flag overhead "o" : Option Minutes` | Optional `--overhead` / `-o`; the handler uses zero when absent |
| `arg tasks : List String` | Zero or more task names, in order |

`Minutes` is a refined `Float` with `Float.is_safe_integer n && n >= 0`;
`deriving Cli.Value` reuses the CLI parser and checks that predicate. Missing
budget, negative or fractional minutes, infinity and unsafe integers fail
before the planner runs. `--help` and `plan --help` also run no planner.
The `--` in the commands above separates compiler arguments from application
arguments.

Try a short alias, an empty plan, and deliberately invalid input:

```sh
./target/debug/flick run --project examples/mtl -- plan -b 36 -o 3 docs lint
./target/debug/flick run --project examples/mtl -- plan --budget 0
./target/debug/flick run --project examples/mtl -- plan --budget 1.5 docs
```

The empty plan succeeds with all zero minutes still available; the fractional
budget fails with CLI help and exit code 1. The CLI shell uses `Cli.run`,
`Platform` and the Effect-backed runtime. Only the domain stack below has no
`Eff` base; this is **not a dependency-free CLI**.

## Six small representations

The implementations live in [`src/Mtl`](src/Mtl/). These are ordinary user-defined
types and Prelude instances, not compiler-recognized transformer types.

| Type | What its constructor holds | What it adds |
| --- | --- | --- |
| [`Identity a`](src/Mtl/Identity.fl) | `a` | A pure base, with no extra context |
| [`MaybeT m a`](src/Mtl/MaybeT.fl) | `m (Option a)` | Stop on `None`; continue on `Some a` |
| [`StateT s m a`](src/Mtl/StateT.fl) | `s -> m (a, s)` | Thread an evolving state through actions |
| [`ReaderT r m a`](src/Mtl/ReaderT.fl) | `r -> m a` | Supply a shared read-only environment |
| [`WriterT w m a`](src/Mtl/WriterT.fl) | `m (a, w)` | Accumulate ordered monoidal output |
| [`ExceptT e m a`](src/Mtl/ExceptT.fl) | `m (Result e a)` | Stop on a domain `Err`; continue on `Ok a` |

Here `m : Type -> Type` is the inner computation type. For example, fixing the
state and inner computation gives `StateT Float Identity : Type -> Type`, which
can itself be the `m` of another transformer.

Each module exports its constructor and a `run` function. `run` removes **one**
wrapper; it is not a universal I/O runner. `StateT.run` also needs an initial state,
`ReaderT.run` needs an environment, and `Identity.run` returns the final plain value.

### State: read, replace, thread

Import operations from `Mtl.Classes`, not from individual transformer modules:

```flick
import Mtl.Classes exposing (lift, get, put, state, ask, local, reader, tell, listen, pass, writer, throw_error, catch_error)
```

```flick
tick : StateT Float Identity Float
let tick = do {
  before <- get
  put (before + 1)
  pure before
}
```

`get` reads without changing state. `put` replaces it and returns
`Unit`. Running `Identity.run (StateT.run tick 10)` produces `(10, 11)`.
To implement `flat_map`, run the first transition, unpack `(value, next)`, and run
the callback's transition with `next`. Mapping changes only the value, not the
state returned alongside it.

### Reader: ask and temporarily change the environment

```flick
reader_example : ReaderT Float Identity (Float, Float, Float)
let reader_example = do {
  before <- ask
  inside <- local (fun n -> n + 10) ask
  after <- ask
  pure (before, inside, after)
}
```

`Identity.run (ReaderT.run reader_example 5)` produces `(5, 15, 5)`.
`local` transforms the environment only for its nested action; there is no mutable
environment to restore. `flat_map` runs both the first action and its continuation
with the same supplied environment. Mapping transforms the inner result.

### Maybe: stop without confusing absence with an inner failure

`MaybeT.from_option (Some value)` continues with `value`;
`MaybeT.from_option None` stops the enclosing workflow. `flat_map` unwraps the
inner action and inspects its option: return `pure None`, or run the callback on
the value inside `Some`. Mapping changes only the value inside `Some`.

`lift action : MaybeT m a` maps `Some` over an existing inner action. It does
**not** turn an error from that inner computation into `None`. For example,
with `m = Result String`, lifting `Err "offline"` runs to `Err "offline"`,
whereas embedding `None` with `from_option` runs to `Ok None`.

## Build the planner stack

[`App.Plan`](src/App/Plan.fl) defines:

```flick
type alias Planner a = MaybeT (StateT PlanState (ReaderT Config Identity)) a
```

`Config` holds `budget` and `overhead`; `PlanState` holds `remaining` and
`completed`. The outer `MaybeT` controls whether scheduling continues, `StateT`
records accepted tasks, and `ReaderT` supplies configuration to every step.
`Identity` means the domain computation needs no `Eff` base.

Capability instances route operations through the stack automatically:

```flick
// Inside App.Plan's Planner do block:
config <- ask
state <- get
```

`MonadReader m r | m -> r` and `MonadState m s | m -> s` let the known
`Planner` type determine the environment and state. Context-assisted FD coverage
admits their inherited instances; instance selection still uses heads alone.
The outermost layer with the native capability wins. To reach a repeated deeper
capability deliberately, give the inner action a type and use the shared `lift`
once for each intervening layer.

Scheduling looks up the task, uses `from_option` to reject an unknown name or an
insufficient budget, then uses `put` to record success.
The recursive workflow sequences these actions with ordinary `do` notation.
There is no planner-specific `do` implementation.

At the domain boundary, unwrap in the opposite direction, outside first:

```flick
Identity.run (ReaderT.run (StateT.run (MaybeT.run (workflow tasks)) initial) config)
```

Read the expression from the inside out:

1. `MaybeT.run` exposes `StateT PlanState (ReaderT Config Identity) (Option Unit)`.
2. `StateT.run ... initial` exposes `ReaderT Config Identity (Option Unit, PlanState)`.
3. `ReaderT.run ... config` exposes `Identity (Option Unit, PlanState)`.
4. `Identity.run` returns `(Option Unit, PlanState)`.

`App.Plan.plan` maps a successful result to the completed task list and returns
`(Option (List String), PlanState)`. It is a pure function of configuration and
task names. [`App.Main`](src/App/Main.fl) is the I/O shell: `Cli` parses arguments,
`Platform` prints the result, and `App.fail` reports an unsuccessful plan. That
shell still uses the Effect-backed runtime; a pure domain is not a claim that the
application or its dependencies are Effect-free.

### Order changes what survives failure

For `MaybeT (StateT s Identity) a`, unwrapping yields `(Option a, s)`.
The state is outside the option, so even `None` comes with the state already
reached. The planner deliberately retains the completed prefix. This is not a
rollback or transaction: a state change made *before* failure also survives.

For `StateT s (MaybeT Identity) a`, unwrapping instead yields `Option (a, s)`.
When the result is `None`, there is no returned pair, so callers cannot recover
the updated state from it. This does not undo arbitrary effects in an inner
computation; it changes which state is present in the result.

[Behavior tests](test/Mtl/TransformersTest.fl) demonstrate both orders, skipped
later actions, scoped `local`, and a cold `Eff` base. [Planner tests](test/App/PlanTest.fl)
cover exact fits, overhead, unknown tasks, over-budget tasks and repetitions.

## Write the instances, not special syntax

All six modules implement the Prelude's `Functor`, `Applicative` and `Monad`:
`map` transforms results, `pure` embeds a value, `map2` combines two actions, and
`flat_map` chooses the next action using a result. Sequential `do` uses those
operations; an `and` group uses `map2` and does not by itself promise concurrency.

The constraints follow the representation:

- **Identity:** unwrap, apply the callback and wrap; no inner constraint.
- **MaybeT:** mapping needs `Functor m`, while `from_option` needs
  `Applicative m`. Both `map2` and `flat_map` need `Monad m` to inspect the first
  option before deciding whether to run the next action. Even an `and` group
  short-circuits left to right here.
- **StateT:** mapping needs `Functor m`, `pure` needs `Applicative m`, and
  `map2` and `flat_map` need `Monad m`. `map2` passes the left action's final state
  to the right action.
- **ReaderT:** mapping needs `Functor m`, `pure` and `map2` need
  `Applicative m`, and `flat_map` needs `Monad m`. `map2` gives both operands the
  same environment and delegates to the inner `map2`.
- **WriterT:** mapping needs `Functor m`; Applicative needs `Monoid w` and
  `Applicative m`; Monad needs `Monoid w` and `Monad m`. Both combinations append
  left output before right output. `map2` retains the inner Applicative's
  execution semantics.
- **ExceptT:** mapping needs `Functor m`; Applicative and Monad need `Monad m`.
  A left `Err` skips the right action or continuation. A base failure stays a
  base failure, distinct from a returned domain `Err`.

The single `MonadTrans.lift` requires `Monad m` for every transformer; WriterT
also requires `Monoid w`. Capability operations require their `Monad`
superclass. These shared contracts intentionally replace the formerly weaker
module-specific operations, without compatibility aliases.

The [law tests](test/Mtl/TransformersTest.fl) check the custom instances, and
[`LawsTest`](test/Mtl/LawsTest.fl) connects the same Prelude laws and notation to
built-in instances. Start a new wrapper with its representation and decide how
its context flows through these four operations before copying an instance.

`LawsTest` covers Option, List, Result, Seq, Validation and the monoidal wrappers;
`TransformersTest` covers Identity and the existing three transformers;
[`WriterExceptTest`](test/Mtl/WriterExceptTest.fl) covers the new two.
[`CapabilitiesTest`](test/Mtl/CapabilitiesTest.fl) exercises inherited/native
operations, defaults and ordering. [`EffLawsTest`](test/Mtl/EffLawsTest.fl)
uses effectful `Arbitrary.check` properties with seed 7 and 100 runs, comparing
complete outcomes and ordered traces rather than executing effects synchronously.

```sh
./target/debug/flick test --project examples/mtl --profile debug --seed 7 --jobs 1
./target/debug/flick test --project examples/mtl --profile release --seed 7 --jobs 1
```

### Capabilities and ordering

[`Mtl.Classes`](src/Mtl/Classes.fl) defines the shared API. Each transformer's
module owns its instances and imports no sibling transformer.

| Outer layer | State | Reader | Writer | Error |
| --- | --- | --- | --- | --- |
| MaybeT | inherited | inherited | inherited | inherited |
| StateT | native | inherited | inherited | inherited |
| ReaderT | inherited | native | inherited | inherited |
| WriterT | inherited | inherited | native | inherited |
| ExceptT | inherited | inherited | inherited | native |

`state`, `reader` and `writer` have one-directional class defaults, and the native
instances use direct implementations. `listen` returns and preserves output
without duplicating it. `pass` changes only its nested action's output; its
modifier has the stored rank-1 type `(w -> w)^c`, not an impredicative `forall`
inside a tuple. None/Err branches of lifted `pass` preserve output via identity.
`catch_error` takes the handler first and does not catch the handler's own failure.
It catches domain errors, not defects, interruption or a typed failure of `Eff`.

Order determines retained data: StateT over ExceptT restarts a catch handler
from the caught action's entry state; ExceptT over StateT preserves state changes.
WriterT over ExceptT loses a failed action's output pair; ExceptT over WriterT
preserves its log. `local f (catch_error h action)` also affects the handler;
`catch_error h (local f action)` does not.

[`App.Capabilities`](src/App/Capabilities.fl) uses a capability-polymorphic
workflow over `ExceptT String (WriterT (List String) (StateT Float
(ReaderT Float m)))`. It runs independently over Identity and a cold Eff base,
checking the lifted effect executes exactly once:

```text
identity ok=5 state=15 log=begin,done
identity error=negative state=10 log=begin
eff ok=5 state=15 log=begin,done
eff error=negative state=10 log=begin
```

### Read the capture annotations

The explicit instance signatures preserve resource captures; these symbols are
not extra runtime arguments:

- `c` and `d` are capture rows. In these instance methods, `c` bounds the callback's
  stored captures and `d` bounds the input computation's captures. `^(c + d)`
  allows the result to retain both. In `pure`, `c` instead names the input value's
  bound. Letters are local names, not fixed meanings across all signatures.
- `a^v` bounds the captures of the actual value passed to the callback.
  `(forall v. a^v -> (m b)^(c + v))^c`, the shape used by `flat_map`, is a
  **rank-2 callback**: the inner `forall` requires the callback to work for each
  invocation's input capture row. Its returned action may retain both that input
  and the callback's own captures.
- The invocation row `v` is independent of the stored computation row `d`.
  A State action may receive state, or a Reader action its environment, only
  later at `run`. Those supplied values can carry captures absent when the
  action was constructed. Do not replace the callback's `v` with `d` or pretend
  `d` bounds all future state and environment inputs.
- `map2` quantifies two invocation rows, `u` and `v`. Its second curried callback
  stage has `^(c + u)` because it retains the callback and its first argument.
  Keep these qualifiers when writing the method signature, rather than erasing
  captures to make a wrapper appear unrestricted.

See [Regions and captures](../../docs/src/content/docs/reference/regions.md) for
the full rules, including the distinction between whole-value bounds and hidden
captures of delayed functions and wrappers.
