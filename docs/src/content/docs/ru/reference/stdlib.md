---
title: "Стандартная библиотека"
---

Стандартная библиотека написана на Flick и использует адаптер среды выполнения
(см. [Функции высшего порядка и обход коллекций](../../spec/06-higher-order/), [Schema и стандартные типы данных](../../spec/09-schema/), [CLI Flick](../../spec/12-cli/)).
Её модули встроены в компилятор. К ним можно обращаться по полному имени
без импорта (`List.map`, `Ref.make`) или импортировать отдельные имена,
как из любого другого модуля (`import File exposing (FileSystem, OpenError)`).
Имена из Prelude доступны в каждом модуле без квалификатора.
Сервис или тип, объявленный в программе, затеняет операции одноимённого
встроенного модуля: если объявлен `service Log`, то `Log.write` —
операция этого сервиса.

Сигнатуры в библиотеке дословно совпадают со спецификацией.
Это проверяет `flick-core/tests/stdlib_signatures.rs`.

## Prelude

| Имя | Назначение |
| - | - |
| `Eq`, `Ord`, `Hash`, `Show` | каноническое равенство, полный порядок, хеширование, представление для вывода |
| `Functor`, `Foldable`, `Traversable`, `Applicative`, `Monad` | классы для конструкторов типов `f : Type -> Type`: `map`, `fold` и `fold_map`, `traverse`, `pure` и `map2`, `flat_map` |
| `Semigroup`, `Monoid`, `Alternative` | `combine` и `empty` (`String`, `List a`, обёртки из `Monoids`), `nothing` и `or_else` |
| `bind`, `bind_eff` | последовательное выполнение для любой монады и для `Eff` с объединением строк |
| `from_result` | `Result (Errors e) a ->{; e} a` |
| `parallel`, `parallel_all`, `parallel_results`, `parallel_exits`, `race_success`, `race_first`, `timeout`, `timeout_option` | структурированная конкурентность, см. [среду выполнения](../runtime/) |
| `Failure`, `Exit`, `RawCause`, `CauseReport` | непрозрачные исходы выполнения |

Для `Option`, `Result`, `List` и `Eff r e` определены подходящие им
экземпляры классов из списка `Functor`, `Foldable`, `Traversable`
(последовательный), `Applicative` и `Monad`.

## Модули

| Модуль | Содержимое |
| - | - |
| `List` | `map` (принимает чистую функцию), `map_effect` (принимает эффектную функцию и обрабатывает элементы последовательно), `map_indexed`, `each_effect`, свёртки, `filter`, `find`, `sort` (стабильная сортировка по `Ord`), `sort_by`, `get`, `slice`, `range`, `zip`, … |
| `Seq` | ленивые последовательности, каждая итерация начинается заново: бесконечные `from`, `iterate`, `repeat`, `cycle`, `unfold`; `range`, `map`, `filter`, `flat_map`, `take`, `take_while`, `drop`, `zip`, `enumerate`, `scan`, `chunks`, `windows`, `find`, `first`, `to_list`, `fold`, `each_effect`; `Monad` для `do`; `Stream.from_seq` превращает её в поток |
| `Monoids` | `Sum` и `Product` чисел, `First` и `Last` от `Option`, `Min` и `Max` по `Ord`: `fold_map Sum xs` говорит, как складывать числа |
| `Validation` | `Validation e a` — `Result`, у которого `map2` объединяет ошибки обоих значений через их `Semigroup`: группа `and` в `do` сообщает обо всех ошибках; `valid`, `invalid`, `from_result`, `to_result` |
| `Form` | формы типа-записи с `deriving Form`: `draft`, `input`, `validate`, `report` (см. [Формы](../form/)) |
| `Route` | URL типа с `deriving Url`: `print` и `parse`; текущую страницу держит `Atom.route` (см. [Маршруты](../route/)) |
| `Option`, `Result` | `map`, `bind`, `with_default` и преобразования |
| `NonEmpty` | списки как минимум с одним элементом |
| `Chunk` | `Chunk` из Effect для многократной конкатенации |
| `Js.Array` | изменяемые массивы JS, доступные через эффектные операции; списки копируются на входе и выходе |
| `String` | `split`, `join`, `slice`, `index_of`, `to_number`, … над кодовыми единицами UTF-16 |
| `Int` | `to_float`, `from_float`, `truncate`, `parse`, побитовые `bit_*` и `shift_*`, проверяемые `checked_add`, `checked_div`, …, возвращающие `Result ArithmeticError Int`, и `unsafe_div`, `unsafe_rem` |
| `Float` | `strict_equal` (`===`), `same_value` (`Object.is`), округление, `parse`, `is_nan`, `is_finite`, … |
| `BigInt` | преобразования и проверяемые `divide`, `remainder` |
| `Ref` | `make`, `get`, `set`, `modify` и `same` для проверки идентичности; без `Eq` и `Hash` |
| `Redacted` | эффектные `make`, `reveal`; `show` выводит `<redacted>` |
| `BigDecimal` | `BigDecimal` из Effect: `from_string`, точные `add`, `subtract`, проверяемые `multiply`, `divide_to_scale` |
| `Duration` | `Duration` из Effect: проверяемые `nanos`, `millis`, `seconds`, `minutes`; для неограниченной длительности — явное `infinity` |
| `EqHash`, `HashSet`, `HashMap` | хеш-коллекции, сохраняющие своё свидетельство |
| `Loop` | `loop` над `Step` и `Trampoline` для чистой рекурсии без переполнения стека |
| `Platform` | `stdout_line`, `stderr_line`, `stdout_write`, `args`, `env` |
| `Log` | сервис `Log` с `info`, `warn`, `error`, `debug` и `Log.console` |
| `File` | сервис `FileSystem`: `open_scoped`, `read_all`, `read_text`, `write_text` и `File.node` |
| `ChildProcess` | дочерние процессы, как в Effect: команда `Command` (`make`, `set_cwd`, `set_env`, `set_stdin`, `prefix`, `pipe_to`, `inherit_output`) выполняется до конца через `string`, `lines` и `exit_code` или запускается `spawn` как `Process` scope'а, с `stdout`, `stderr`, `wait`, `is_running`, `kill` и `kill_with` |
| `Resource` | `acquire_release` с передачей ресурса владельцу области и `bracket` |
| `Events` | политики `queue` и супервизор `log_failures` |
| `Ui`, `Ui.Event` | пользовательские интерфейсы, отображаемые с помощью React, см. [пользовательские интерфейсы](../ui/) |
| `Sql`, `Database` | типизированные запросы PostgreSQL и сервис `Database`, см. [запросы SQL](../sql/) |
| `Rpc` | типизированные удалённые вызовы: процедуры с кодеками, группы, серверы и клиенты поверх HTTP, WebSocket, TCP и стандартных потоков, см. [типизированные RPC](../rpc/) |
| `Metric`, `Tracer` | метрики реестра программы и спаны вокруг её задач, см. [наблюдаемость](../observability/) |
| `Otlp`, `Prometheus`, `DevTools` | куда уходят спаны, записи журнала и метрики: коллектор OTLP, маршрут Prometheus, Effect DevTools, см. [наблюдаемость](../observability/) |
| `JsError` | `JsError.from_js`, ошибка внешнего исключения или отклонения промиса, см. [внешние функции](../ffi/) |
| `Fiber`, `Failure`, `Exit`, `RawCause` | дочерние задачи области и исходы выполнения |
| `Test`, `Gen` | тесты и генераторы, см. ниже |
| `Schedule` | расписания Effect: `recurs`, `spaced`, `exponential`, `fibonacci`, `up_to`, `while`, `tap`, … |
| `Effect` | операции Effect, дополняющие встроенные возможности языка: `retry` (только типизированные ошибки), `repeat`, `schedule`, `sleep`, … |

У каждого экспорта закреплённой версии Effect есть обёртка или другое
явно записанное решение в манифесте. Это проверяет линтер;
см. [покрытие Effect](../effect-surface/).

## Числа

`Ord Float` задаёт полный порядок: `-Infinity` < конечные числа < `Infinity` <
`NaN`. Нули с разными знаками равны, как и все значения NaN.
Поэтому `List.sort` помещает `NaN` в конец.
Операторы `<`, `<=`, `>`, `>=` сохраняют семантику JS.

`BigDecimal.divide_to_scale scale mode dividend divisor` вычисляет
частное с `scale` знаками после десятичной точки, используя коэффициенты
типа BigInt. Глобальная настройка точности не нужна:

| `mode` | `5 / 2` | `-5 / 2` | `11 / 4` | `-11 / 4` |
| - | - | - | - | - |
| `TowardZero` | 2 | -2 | 2 | -2 |
| `Floor` | 2 | -3 | 2 | -3 |
| `Ceiling` | 3 | -2 | 3 | -2 |
| `HalfEven` | 2 | -2 | 3 | -3 |
| `HalfAwayFromZero` | 3 | -3 | 3 | -3 |
| `Exact` | `DecimalError` | `DecimalError` | `DecimalError` | `DecimalError` |

Если делитель нулевой или масштаб не является безопасным целым числом,
функция тоже возвращает `DecimalError`. Вызовы `Duration.millis (-1)`,
`Duration.millis Float.nan` и преобразование с переполнением дают
`Err (DurationError …)`. Неограниченная длительность задаётся как
`Duration.infinity`; для неё `Duration.to_millis` возвращает `None`.

## Хеш-коллекции

`HashSet w a` или `HashMap w k v` хранит свидетельство равенства и хеширования,
с которым создана коллекция. Идентичность этого свидетельства `w` входит
в тип коллекции. Вызов `HashSet.empty` использует канонические экземпляры
классов (`w` — это `EqHash.Canonical`). Вызов `EqHash.with eq
hash body` создаёт свидетельство с новой идентичностью и передаёт его в `body`.
Из `body` нельзя вернуть само свидетельство или значение, в типе которого упоминается его идентичность.
`HashMap.from_seq` и `HashSet.from_seq` строят коллекцию из последовательности,
например из `for`, а `to_seq` возвращает элементы как источник для `for`.

Вставка и поиск используют свидетельство самой коллекции. Вызов `HashSet.union`
для множеств с разными свидетельствами приводит к ошибке типов.
Перед объединением нужно перевести множество на другое свидетельство
с помощью `HashSet.rehash witness set`.

```flick
case_blind : Float
let case_blind =
  EqHash.with
    (fun a b -> String.to_lower a == String.to_lower b)
    (fun s -> hash (String.to_lower s))
    (fun witness -> HashSet.size (HashSet.insert "ADA" (HashSet.insert "ada" (HashSet.with witness))))
```

## Дочерние процессы

`stdout` и `stderr` процесса — это pipe. Что происходит с выводом, который
программа не читает, на двух target разное:

| | JS-target (Node) | Go-target |
| - | - | - |
| непрочитанный вывод | Node приостанавливает поток; когда буфер pipe (около 64 КиБ) полон, ребёнок ждёт, пока вывод прочтут | читается сразу и копится в памяти; ребёнок не ждёт |
| цена | ребёнок с большим выводом может встать навсегда | память растёт с объёмом вывода, пока не закроется scope процесса |

Примеры:

```flick
// a. spawn и только wait: 1 МиБ, который никто не читает
scope {
  let p = ChildProcess.spawn (ChildProcess.make "sh" ["-c", "head -c 1048576 /dev/zero"])
  ChildProcess.wait p // JS: не возвращается; Go: 0
}

// b. exit_code команды с большим выводом: так же
ChildProcess.exit_code (ChildProcess.make "sh" ["-c", "head -c 1048576 /dev/zero"])

// c. бесконечный непрочитанный вывод, например `yes`: на JS ребёнок встаёт;
//    на Go всё написанное держится в памяти до `kill` или закрытия scope
```

Читайте то, что пишет ребёнок (`stdout`, `stderr`, `string`, `lines`), или
отдайте ему потоки программы через `inherit_output` — тогда target ведут
себя одинаково. Поведение Go закреплено тестом
`TestChildProcessUnreadOutputDoesNotBlock` в
`runtime-go/flickrt/process_test.go`.

## Тесты

Модуль из корневого каталога тестов экспортирует `tests : List Test.Test`:

```flick
module Test.Text exposing (tests)

tests : List Test.Test
let tests =
  [
    Test.test "trims" (fun () -> Test.equal "a" (String.trim " a ")),
    Test.effect_test "reads" (task { Test.expect (Test.equal 1 1) }),
    Test.property "reverse twice" (Gen.list 10 (Gen.int 0 9)) (fun xs ->
      Test.equal xs (List.reverse (List.reverse xs))),
    Test.group "numbers" [Test.test "adds" (fun () -> Test.equal 2 (1 + 1))],
  ]
```

| Функция | Тест |
| - | - |
| `Test.test name check` | чистый тест: `check ()` возвращает `Assertion` |
| `Test.effect_test name task` | выполняет задачу в собственной области, которая владеет полученными ресурсами; `Test.expect` завершает задачу с ошибкой, если утверждение не выполнено |
| `Test.clock_test name task` | как `effect_test`, но на собственных часах, которые начинаются с 0 и идут только от `Test.adjust duration` и `Test.set_time millis`: сон, таймаут или расписание повторов ждут виртуального времени, так что тест часового backoff проходит сразу |
| `Test.property name gen check` | выполняет `check` на 100 значениях `gen`, полученных из начального значения генератора данного запуска; `Gen.function result` (для аргумента с `Hash`) порождает чистые функции, нужные законам над функциями |
| `Test.group name tests` | добавляет `name >` перед именами из `tests` |

`flick test` собирает проект вместе с корневым каталогом тестов в `.flick/test/`
и запускает все тесты. Полное имя теста имеет вид `Module > group > name`.

| Параметр | Значение |
| - | - |
| `--filter <text>` | только тесты, полное имя которых содержит `text` |
| `--jobs <n>` | не более `n` тестов одновременно, по умолчанию `1` |
| `--seed <n>` | начальное значение генераторов; неудачный запуск выводит использованное значение |
| `--reporter human\|json` | отчёт со строкой для каждого теста и сводкой либо один объект JSON |
| `--watch` | повторный запуск при изменении файла проекта |
| `--emit js\|go` | запуск на Node или одним исполняемым файлом Go, `.flick/test/flick-test`; по умолчанию по `build.emit` |

Результат каждого теста — `passed` или `failed`. При сбое указывается причина:
`assertion`, `failure` (типизированная ошибка), `defect`, `cleanup`
(область теста не закрылась корректно; даже если тело выполнилось успешно,
тест считается неудачным) или `interrupted`.
Код завершения — `0`, если прошли все тесты, иначе `1`.

Сигнал SIGINT прерывает выполняемые тесты. Каждый из них закрывает свою область
до завершения средства запуска. В режиме `--watch` изменение файла останавливает
текущий запуск тем же способом: наблюдатель закрывает stdin средства запуска,
что работает на всех платформах. Следующий запуск начинается только после
завершения текущего.
