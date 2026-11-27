# Dependent choices and a cloud release configurator (`monad-choice`)

A port of Hackage's [`monad-choice`](https://hackage.haskell.org/package/monad-choice)
to Flick, paired with a cloud release configurator where one polymorphic
workflow runs in five modes: exhaustive matrix enumeration, constraint search
with chronological backtracking, reproducible random sampling, interactive
terminal menus and property-test generation.

| Part | Where |
| - | - |
| `Choices a`: non-empty `Choices first rest` with capture-preserving `Monad` | `src/Choice/Choices.fl` |
| `MonadChoice f m`, `choose`, `choose_m` and `MonadTrans` | `src/Choice/Class.fl` |
| covariant `ChoiceT` GADT with existential choice nodes and `run_backtrackable_choice_t` | `src/Choice/Covariant.fl` |
| invariant `ChoiceT` over rank-2 CPS `((forall x. f x -> m x) -> m a)` and `invmap_choice_t` | `src/Choice/Invariant.fl` |
| `UniformRandom` over `Host.Random` and `RollChoice` for `Gen` / `Arbitrary` | `src/Choice/Random.fl` |
| `StateT` lifting `MonadChoice` and driving backtracking cursors | `src/Choice/StateT.fl` |
| release domain (`Topology`, `Storage`, `Cache`, `Rollout`) and architecture rules | `src/Deploy/Domain.fl` |
| `Question a` functor carrying prompts and option metadata | `src/Deploy/Question.fl` |
| `berry` (from `monad-choice`'s README) and `blueprint` | `src/Deploy/Blueprint.fl` |
| five interpreters over `blueprint` (`enumerate`, `solve`, `sample_seeded`, `interactive`, `plan_generator`) | `src/Deploy/Interpreters.fl` |
| typed `cli` with `matrix`, `solve`, `sample` and `wizard` | `src/App/Main.fl`, `src/App/View.fl` |

## Run it from the repository root

Use the checkout's [source setup](../../README.md#build-from-source), including
the pinned Rust toolchain, Node.js and pnpm:

```sh
pnpm install --frozen-lockfile
pnpm -C runtime build
pnpm -C packages/effect-ts build
cargo build -p flick

./target/debug/flick run --project examples/monad-choice -- matrix --tier production
./target/debug/flick run --project examples/monad-choice -- matrix --tier preview
./target/debug/flick run --project examples/monad-choice -- solve --tier production --max-cost 450 --max-latency 35 --min-sla 9995 --zero-downtime
./target/debug/flick run --project examples/monad-choice -- sample --tier production --seed 42 --count 3
./target/debug/flick run --project examples/monad-choice -- wizard --tier production
./target/debug/flick test --project examples/monad-choice
./target/debug/flick test --project examples/monad-choice --target go
```

If `flick` is already on your `PATH`, replace `./target/debug/flick` with `flick`.
`solve` with the flags above prints [`expected.stdout`](expected.stdout) on both
Node and Go:

```text
Constraints (production): budget <= $450/mo, p95 <= 35ms, SLA >= 99.95%, zero-downtime: required
Solver: explored 50 choice steps, backtracked 46 times
Selected: regional-3az / postgres-ha-r2 / redis-ha / canary-10pct
Metrics: $440/mo | p95 30ms | SLA 99.95% | zero-downtime
```

When no valid configuration satisfies the requested constraints (for example
`solve --tier production --max-cost 100 --zero-downtime`), the command prints
how many choice steps and backtracks were explored and exits with code 1.

## Why `ChoiceT` complements Flick's native effect handlers

Flick's built-in `effect` handlers (`control` and `resume`) are intentionally
**one-shot**: resuming a continuation a second time is a defect, so regional
resources in `scope` are acquired and released once per fiber.

`ChoiceT` instead reifies dependent choices of arbitrary types as pure
values. Each step can choose a different type, and later choices can inspect
earlier answers before building the next option set:

```flick
# type Choices a = Choices a (List a)
# class Monad m => MonadChoice (f : Type -> Type) (m : Type -> Type) =
#   choose : forall c a. (f a)^c -> (m a)^c
berry : forall m. MonadChoice Choices m => m String
let berry = do {
  color <- choose (Choices "red" ["blue", "orange", "yellow", "black"])
  flavor <- choose (Choices "sweet" ["sour", "bitter"])
  stem <- choose (Choices color [flavor, "\{color}-\{flavor}"])
  pure "\{stem}berry"
}
```

In `Deploy.Blueprint.blueprint`, the four `choose` steps pick `Topology`,
`Storage`, `Cache` and `Rollout`. Incompatible combinations (such as
`EmbeddedSqlite` on multi-zone topologies, `LocalMemory` cache across regions,
or `BlueGreen` cutovers over a single-writer SQLite file) are never offered in
the first place.

## Two representations of `ChoiceT`

`monad-choice` ships two monad transformers, and Flick types both cleanly:

1. **Covariant GADT (`Choice.Covariant`)** hides the intermediate choice type `b`
   behind `LiftThenT` and `ChooseThenT`:

   ```flick
   type ChoiceT (f : Type -> Type) (m : Type -> Type) a =
     | FixedT a
     | LiftThenT (m b) (b -> ChoiceT f m a) : ChoiceT f m a
     | ChooseThenT (f b) (b -> ChoiceT f m a) : ChoiceT f m a
   ```

   Because `m` appears only positively, `ChoiceT f m` is a `Functor`,
   `Applicative` and `Monad` without any constraint on `f` or `m`, supports
   `map_choice_t`, and exposes choice nodes to `run_backtrackable_choice_t`.

2. **Invariant rank-2 CPS (`Choice.Invariant`)** represents a computation by its
   eliminator:

   ```flick
   type ChoiceT (f : Type -> Type) (m : Type -> Type) a =
     ChoiceT ((forall v x. (f x)^v -> (m x)^v) -> m a)
   ```

   Here `m` appears in both negative (`f x -> m x`) and positive (`-> m a`)
   positions, so transforming the base monad requires `invmap_choice_t` with
   maps in both directions.

## Backtracking without rebuilding earlier prefixes

`Covariant.run_backtrackable_choice_t` takes a chooser returning
`m (Option x)`. Returning `None` unwinds to the nearest enclosing
`ChooseThenT` node and re-runs it. `Deploy.Interpreters.solve` pairs this with
`StateT CursorState Identity`: returning `Some` advances the cursor at the
current depth and descends, while returning `None` truncates exhausted deeper
cursors and ascends one level. Even on variable-depth choice trees, each branch
is visited at most once.

## Tests

- `test/Choice/ChoiceTest.fl` and `test/Choice/ChoiceLawsTest.fl`:
  - `berry` produces all 45 berry names identically under `Covariant` and `Invariant`;
  - `choose_m`, `StateT` lifting, variable-depth backtracking and cold `Eff` lifting;
  - property checks for `Functor`, `Applicative`, `Monad`, `invmap_choice_t` and `low_map` laws.
- `test/Deploy/DeployTest.fl` and `test/Deploy/DeployPropertyTest.fl`:
  - `Covariant` and `Invariant` produce identical 26-plan preview and 48-plan production matrices;
  - every generated plan satisfies `Domain.valid_architecture`;
  - property check comparing `solve` (`run_backtrackable_choice_m`) against linear search over `enumerate` across random constraint sets;
  - `Gen`, `Arbitrary` (100 shrinking runs) and seeded `UniformRandom` sampling checks.
- `test/App/WizardTest.fl`:
  - scripted `Cli.Prompt.Tty` keypress tests verifying default selection, dynamic menu adaptation for `GlobalEdge` + `DistributedSql`, SQLite rollout restriction in preview, and `Terminal.QuitError` on early exit.
