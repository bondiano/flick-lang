# Зависимые выборы и конфигуратор облачного релиза (`monad-choice`)

Порт библиотеки [`monad-choice`](https://hackage.haskell.org/package/monad-choice)
с Hackage на Flick и прикладной конфигуратор облачного релиза, в котором одно
полиморфное описание запускается сразу в пяти режимах: полный перебор матрицы
стендов, поиск по ограничениям с хронологическим откатом, воспроизводимый
случайный выбор по сиду, интерактивный мастер в терминале и генератор для
property-тестов.

| Часть | Где |
| - | - |
| `Choices a`: непустой список `Choices first rest` с сохранением захватов в `Monad` | `src/Choice/Choices.fl` |
| `MonadChoice f m`, `choose`, `choose_m` и `MonadTrans` | `src/Choice/Class.fl` |
| ковариантный GADT `ChoiceT` с экзистенциальными узлами выбора и `run_backtrackable_choice_t` | `src/Choice/Covariant.fl` |
| инвариантный `ChoiceT` поверх rank-2 CPS `((forall x. f x -> m x) -> m a)` и `invmap_choice_t` | `src/Choice/Invariant.fl` |
| `UniformRandom` поверх `Host.Random` и `RollChoice` для `Gen` / `Arbitrary` | `src/Choice/Random.fl` |
| `StateT` с подъёмом `MonadChoice` и курсорами отката | `src/Choice/StateT.fl` |
| предметная модель релиза (`Topology`, `Storage`, `Cache`, `Rollout`) и архитектурные инварианты | `src/Deploy/Domain.fl` |
| функтор `Question a` с заголовками шагов и описаниями вариантов | `src/Deploy/Question.fl` |
| классический `berry` из README `monad-choice` и `blueprint` релиза | `src/Deploy/Blueprint.fl` |
| пять интерпретаторов одного `blueprint` (`enumerate`, `solve`, `sample_seeded`, `interactive`, `plan_generator`) | `src/Deploy/Interpreters.fl` |
| типизированный `cli` с подкомандами `matrix`, `solve`, `sample` и `wizard` | `src/App/Main.fl`, `src/App/View.fl` |

## Запуск из корня репозитория

Подготовьте [сборку из исходников](../../README.md#build-from-source):
закреплённый тулчейн Rust, Node.js и pnpm.

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

Если `flick` уже есть в `PATH`, замените `./target/debug/flick` на `flick`.
Команда `solve` с флагами выше печатает отчёт из [`expected.stdout`](expected.stdout)
одинаково на Node и на Go:

```text
Constraints (production): budget <= $450/mo, p95 <= 35ms, SLA >= 99.95%, zero-downtime: required
Solver: explored 50 choice steps, backtracked 46 times
Selected: regional-3az / postgres-ha-r2 / redis-ha / canary-10pct
Metrics: $440/mo | p95 30ms | SLA 99.95% | zero-downtime
```

Если ни одна допустимая топология не проходит ограничения (например,
`solve --tier production --max-cost 100 --zero-downtime`), программа сообщает
число проверенных узлов выбора и откатов и завершается с кодом 1.

## Зачем `ChoiceT` при наличии встроенных эффектов Flick

Обработчики алгебраических эффектов во Flick (`control` и `resume`) намеренно
**однократные** (one-shot): повторный вызов `resume` считается дефектом, чтобы
региональные ресурсы `scope` открывались и закрывались ровно один раз на файбер.

`ChoiceT` представляет последовательность зависимых выборов произвольных типов
как чистую структуру продолжений. На каждом шаге тип ответа может быть своим,
а доступные варианты следующего шага вычисляются по предыдущим ответам:

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

В `Deploy.Blueprint.blueprint` четыре вызова `choose` последовательно выбирают
`Topology`, `Storage`, `Cache` и `Rollout`. Несовместимые сочетания (например,
`EmbeddedSqlite` в мультизонном кластере, локальный кэш в памяти при геораспределении
или `BlueGreen` поверх однопоточного файла SQLite) отсекаются самой структурой
зависимых вопросов и вообще не могут быть построены.

## Два представления `ChoiceT`

В `monad-choice` есть два трансформера, и система типов Flick выражает оба без обходных путей:

1. **Ковариантный GADT (`Choice.Covariant`)** скрывает промежуточный тип выбора `b`
   в конструкторах `LiftThenT` и `ChooseThenT`:

   ```flick
   type ChoiceT (f : Type -> Type) (m : Type -> Type) a =
     | FixedT a
     | LiftThenT (m b) (b -> ChoiceT f m a) : ChoiceT f m a
     | ChooseThenT (f b) (b -> ChoiceT f m a) : ChoiceT f m a
   ```

   Так как `m` входит только положительно, `ChoiceT f m` реализует `Functor`,
   `Applicative` и `Monad` без ограничений на `f` и `m`, поддерживает
   `map_choice_t` и открывает узлы выбора для отката в `run_backtrackable_choice_t`.

2. **Инвариантный rank-2 CPS (`Choice.Invariant`)** кодирует вычисление через его
   раннер:

   ```flick
   type ChoiceT (f : Type -> Type) (m : Type -> Type) a =
     ChoiceT ((forall v x. (f x)^v -> (m x)^v) -> m a)
   ```

   Здесь `m` стоит и в аргументе (`f x -> m x`), и в результате (`-> m a`),
   поэтому замена базовой монады требует двустороннего `invmap_choice_t`.

## Поиск с откатом без пересчёта префиксов

`Covariant.run_backtrackable_choice_t` принимает функцию выбора с результатом
`m (Option x)`. Возврат `None` откатывает управление к ближайшему внешнему узлу
`ChooseThenT` и запускает его повторно. В `Deploy.Interpreters.solve` эта схема
работает поверх `StateT CursorState Identity`: возврат `Some` сдвигает курсор
текущей глубины и спускается на уровень ниже, а возврат `None` сбрасывает
исчерпанные глубокие курсоры и поднимается на уровень выше. Даже на деревьях
переменной глубины каждая ветка посещается не более одного раза.

## Тесты

- `test/Choice/ChoiceTest.fl` и `test/Choice/ChoiceLawsTest.fl`:
  - `berry` порождает все 45 названий ягод одинаково через `Covariant` и `Invariant`;
  - проверка `choose_m`, подъёма через `StateT`, отката на деревьях разной глубины и ленивого `Eff`;
  - property-тесты законов `Functor`, `Applicative`, `Monad`, `invmap_choice_t` и `low_map`.
- `test/Deploy/DeployTest.fl` и `test/Deploy/DeployPropertyTest.fl`:
  - `Covariant` и `Invariant` строят совпадающие матрицы из 26 планов `preview` и 48 планов `production`;
  - каждый план матрицы удовлетворяет `Domain.valid_architecture`;
  - property-тест эквивалентности `solve` (`run_backtrackable_choice_m`) и линейного поиска по полной матрице на случайных ограничениях;
  - проверки генераторов `Gen`, `Arbitrary` (100 прогонов со сжатием контрпримеров) и детерминированного `UniformRandom`.
- `test/App/WizardTest.fl`:
  - сценарные тесты терминального мастера через подменённый слой `Cli.Prompt.Tty`: выбор по умолчанию, адаптация меню для `GlobalEdge` + `DistributedSql`, ограничение стратегии выкатки для SQLite и выход по `Terminal.QuitError`.
