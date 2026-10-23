---
title: "Стандартная библиотека"
---

Стандартная библиотека — это код Flick поверх адаптера среды выполнения (см. [Функции высшего порядка и обход коллекций](../../spec/06-higher-order/), [Schema и стандартные типы данных](../../spec/09-schema/), [CLI Flick](../../spec/12-cli/)).
Её модули встроены в компилятор: программа обращается к ним по полному имени
без импорта (`List.map`, `Ref.make`) или импортирует из них имена, как
из любого модуля (`import File exposing (FileSystem, OpenError)`).
Prelude доступен каждому модулю без квалификации. Сервис или тип,
объявленный в программе, затеняет операции одноимённого встроенного модуля:
если объявлен `service Log`, то `Log.write` — операция этого сервиса.

Сигнатуры из спецификации объявлены дословно;
это проверяет `flick-core/tests/stdlib_signatures.rs`.

## Prelude

| Имя | Назначение |
| - | - |
| `Eq`, `Ord`, `Hash`, `Show` | каноническое равенство, полный порядок, хеширование, представление для вывода |
| `Functor`, `Foldable`, `Traversable`, `Applicative`, `Monad` | классы над `f : Type -> Type`: `map`, `fold`, `traverse`, `pure`, `flat_map` |
| `bind`, `bind_eff` | последовательное выполнение для любой монады и для `Eff` с объединением строк |
| `from_result` | `Result (Errors e) a ->{; e} a` |
| `parallel`, `parallel_all`, `parallel_results`, `parallel_exits`, `race_success`, `race_first`, `timeout`, `timeout_option` | структурированная конкурентность, см. [среду выполнения](../runtime/) |
| `Failure`, `Exit`, `RawCause`, `CauseReport` | непрозрачные исходы выполнения |

У `Option`, `Result`, `List` и `Eff r e` есть экземпляры подходящих им
классов: `Functor`, `Foldable`, `Traversable` (последовательный), `Applicative`
и `Monad`.

## Модули

| Модуль | Содержимое |
| - | - |
| `List` | `map` (чистая функция обратного вызова), `map_effect` (функция обратного вызова среды выполнения, по одному элементу последовательно), `map_indexed`, `each_effect`, свёртки, `filter`, `find`, `sort` (стабильная сортировка по `Ord`), `sort_by`, `get`, `slice`, `range`, `zip`, … |
| `Option`, `Result` | `map`, `bind`, `with_default` и преобразования |
| `NonEmpty` | списки как минимум с одним элементом |
| `Chunk` | `Chunk` из Effect для многократной конкатенации |
| `Js.Array` | изменяемые массивы JS за операциями среды выполнения; списки копируются на входе и выходе |
| `String` | `split`, `join`, `slice`, `index_of`, `to_number`, … над кодовыми единицами UTF-16 |
| `Number` | `strict_equal` (`===`), `same_value` (`Object.is`), округление, `parse`, побитовые `bit_*` и `shift_*`, проверяемые целочисленные операции `add_int`, `div_int`, …, возвращающие `Result ArithmeticError Number` |
| `BigInt` | преобразования и проверяемые `divide`, `remainder` |
| `Ref` | `make`, `get`, `set`, `modify` и `same` для проверки идентичности; без `Eq` и `Hash` |
| `Redacted` | `make`, `reveal` (в среде выполнения); `show` выводит `<redacted>` |
| `BigDecimal` | `BigDecimal` из Effect: `from_string`, точные `add`, `subtract`, проверяемые `multiply`, `divide_to_scale` |
| `Duration` | `Duration` из Effect: проверяемые `nanos`, `millis`, `seconds`, `minutes`, явное `infinity` |
| `EqHash`, `HashSet`, `HashMap` | хеш-коллекции, сохраняющие своё свидетельство |
| `Loop` | `loop` над `Step` и `Trampoline` для чистой рекурсии, безопасной для стека |
| `Platform` | `stdout_line`, `stderr_line`, `stdout_write`, `args`, `env` |
| `Log` | сервис `Log` с `info`, `warn`, `error`, `debug` и `Log.console` |
| `File` | сервис `FileSystem`: `open_scoped`, `read_all`, `read_text`, `write_text` и `File.node` |
| `Resource` | `acquire_release` с передачей ресурса владельцу области и `bracket` |
| `Events` | политики `queue` и супервизор `log_failures` |
| `Ui`, `Ui.Event` | пользовательские интерфейсы, отображаемые с помощью React, см. [пользовательские интерфейсы](../ui/) |
| `Sql`, `Database` | типизированные запросы PostgreSQL и сервис `Database`, см. [запросы SQL](../sql/) |
| `JsError` | `JsError.from_js`, ошибка внешнего исключения или отклонения промиса, см. [внешние функции](../ffi/) |
| `Fiber`, `Failure`, `Exit`, `RawCause` | дочерние задачи области и исходы выполнения |
| `Test`, `Gen` | тесты и генераторы, см. ниже |
| `Schedule` | расписания Effect: `recurs`, `spaced`, `exponential`, `fibonacci`, `up_to`, `while`, `tap`, … |
| `Effect` | возможности `Effect` из Effect сверх возможностей языка: `retry` (только типизированные ошибки), `repeat`, `schedule`, `sleep`, … |

Каждый экспорт закреплённой версии Effect либо обёрнут, либо имеет решение
в манифесте, который проверяет линтер; см. [покрытие Effect](../effect-surface/).

## Числа

`Ord Number` задаёт полный порядок: `-Infinity` < конечные числа < `Infinity` <
`NaN`, нули с разными знаками равны, как и все значения NaN, поэтому `List.sort` помещает `NaN` в конец.
Операторы `<`, `<=`, `>`, `>=` сохраняют смысл, который они имеют в JS.

`BigDecimal.divide_to_scale scale mode dividend divisor` вычисляет
частное с `scale` знаками после точки из коэффициентов типа BigInt,
без глобальной настройки точности:

| `mode` | `5 / 2` | `-5 / 2` | `11 / 4` | `-11 / 4` |
| - | - | - | - | - |
| `TowardZero` | 2 | -2 | 2 | -2 |
| `Floor` | 2 | -3 | 2 | -3 |
| `Ceiling` | 3 | -2 | 3 | -2 |
| `HalfEven` | 2 | -2 | 3 | -3 |
| `HalfAwayFromZero` | 3 | -3 | 3 | -3 |
| `Exact` | `DecimalError` | `DecimalError` | `DecimalError` | `DecimalError` |

Нулевой делитель и масштаб, не являющийся безопасным целым числом, тоже дают
`DecimalError`. `Duration.millis (-1)`, `Duration.millis Number.nan` и преобразование
с переполнением дают `Err (DurationError …)`; неограниченная длительность —
`Duration.infinity`, для неё `Duration.to_millis` возвращает `None`.

## Хеш-коллекции

`HashSet w a` или `HashMap w k v` сохраняет свидетельство равенства и хеширования,
с которым создана коллекция, а его идентичность `w` — часть типа. `HashSet.empty`
использует канонические экземпляры классов (`w` — это `EqHash.Canonical`); `EqHash.with eq
hash body` создаёт свидетельство с новой идентичностью и передаёт его в `body`,
которая не может вернуть ничего, что его упоминает. Вставка и поиск используют
собственное свидетельство множества, а `HashSet.union` для множеств с двумя
разными свидетельствами — ошибка типов: сначала `HashSet.rehash witness set`
переводит множество на другое свидетельство.

```flick
case_blind : Number
let case_blind =
  EqHash.with
    (fun a b -> String.to_lower a == String.to_lower b)
    (fun s -> hash (String.to_lower s))
    (fun witness -> HashSet.size (HashSet.insert "ADA" (HashSet.insert "ada" (HashSet.with witness))))
```

## Тесты

Модуль из корня тестов экспортирует `tests : List Test.Test`:

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
| `Test.test name check` | чистый: `check ()` — это `Assertion` |
| `Test.effect_test name task` | задача выполняется в собственной области, которая владеет полученными ею ресурсами; `Test.expect` завершает её с ошибкой при невыполнении утверждения |
| `Test.property name gen check` | `check` на 100 значениях `gen`, полученных из начального значения генератора данного запуска |
| `Test.group name tests` | `name >` перед именами из `tests` |

`flick test` собирает проект вместе с корнем тестов в `.flick/test/` и
запускает все тесты; полное имя теста — `Module > group > name`.

| Параметр | Значение |
| - | - |
| `--filter <text>` | только тесты, полное имя которых содержит `text` |
| `--jobs <n>` | не более `n` тестов одновременно, по умолчанию `1` |
| `--seed <n>` | начальное значение генераторов; неудачный запуск выводит использованное значение |
| `--reporter human\|json` | строки для каждого теста и сводка либо один объект JSON |
| `--watch` | повторный запуск при изменении файла проекта |

Каждый результат — `passed` или `failed` с указанием причины сбоя: `assertion`, `failure`
(типизированная ошибка), `defect`, `cleanup` (область теста не закрылась
корректно; тест с успешно выполненным телом тоже считается неудачным) или `interrupted`.
Код завершения — `0`, если прошли все тесты, иначе `1`. SIGINT прерывает выполняемые
тесты, и каждый из них закрывает свою область до завершения средства запуска;
в режиме `--watch` изменение останавливает запуск тем же способом (наблюдатель закрывает
stdin средства запуска, поэтому это работает на всех платформах) и начинает следующий
запуск только после этого.
