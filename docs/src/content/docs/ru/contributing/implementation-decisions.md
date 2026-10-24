---
title: "Решения реализации"
---

## Решения, которые спецификация оставляет реализации

| Вопрос | Решение |
| - | - |
| Parser | Рукописный recursive descent + Pratt для операторов; lossless CST (trivia сохраняются) для `fmt` и LSP |
| Инкрементальность | Без query-фреймворка. Сборка кешируется целиком в `.flick/cache/` по ключу от всех её входов, включая исходники и интерфейсы установленных зависимостей (см. [Выходные файлы и кеш](../../reference/cli/#выходные-файлы-и-кеш)); `.iface` хранит fingerprints интерфейсов, с которыми модуль собран |
| Spans | UTF-8 byte ranges (см. [Общие параметры и диагностика](../../spec/12-cli/#общие-параметры-и-диагностика)); LSP переводит их в UTF-16 |
| Snapshot-тесты | `insta` для diagnostics, IR, JS-вывода (см. [Тестирование](../testing/)) |
| Node host | Node.js 24.18.0 — минимальная поддерживаемая версия, проверяется в `flick doctor` |
| pnpm | Закреплённая версия через `packageManager` в шаблоне проекта (`pnpm@12.14.0`); `flick doctor` сверяет с ней установленный pnpm |
| Effect | Ровно 4.0.5, commit `81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f` (см. [Сложный Cause](../../spec/07-errors-and-concurrency/#сложный-cause-что-обещает-строка-ошибок), [Компилятор](../../spec/11-compiler/) и [Версия Effect](../../reference/runtime/#версия-effect)) |
| Покрытие Effect API | Каждый export `effect`, `effect/cli`, `effect/http`, `effect/http-api` классифицирован в манифесте; линтер в `cargo test` падает на неклассифицированном, устаревшем или изменившем сигнатуру export (см. [Покрытие Effect](../../reference/effect-surface/)) |
| Node-платформа Effect | Серверный HTTP, `Terminal`/`FileSystem`/`Path` для CLI — из `@effect/platform-node`, закреплённого на 4.0.5 вместе с Effect как зависимость `@flick-lang/runtime` |
| PostgreSQL driver | `pg` (node-postgres) — необязательная peer dependency `@flick-lang/runtime`, привязанная через `foreign` в модуле стандартной библиотеки `Database` (`Database.pg`) |
| React | React 19 + `react-dom/client`, необязательные peer dependencies `@flick-lang/runtime` |
| `.d.ts` разрешение | npm-пакет `typescript` в Node helper `@flick-lang/dts-importer`; JSON-запрос в stdin, поверхность пакета в JSON из stdout (см. [Импорт `.d.ts` пакета](../../reference/ffi/#импорт-dts-пакета)) |
| npm scope пакетов | `@flick-lang/*` (`@flick-lang/runtime`, `@flick-lang/dts-importer`); стандартная библиотека входит в компилятор |
| Сгенерированные имена | Словарь экземпляра — `$i_<Класс>_<Тип>`, если его заголовок — имена через пробел (`Show Number`): по такому имени заголовок восстанавливается однозначно; другой заголовок (`Show (List a)`) сохраняет 8-значный hex-суффикс хэша FNV. Теги конструкторов сохраняют 48-битный хэш (`Circle#d930eed2c811`) во всех профилях, в том числе у приватных типов: тег — идентичность значения между пакетами и модулями, `derive`, `show` и `equals` читают его во время выполнения, а debug- и release-сборки должны в нём совпадать. Пересмотреть, если хэш даст заметный вес после gzip |

## Решения по пробелам спецификации

Спецификация фиксирует семантику, но не задаёт ряд поверхностных деталей.
Решения ниже приняты и описаны в справочнике.

| Пробел | Решение |
| - | - |
| Комментарии | `//` строчный, `/* */` блочный с вложенностью, `///` doc-comment для `flick doc`. Внутри `sql { }` тоже `//` (см. [Пробельные символы и комментарии](../../reference/grammar/#пробельные-символы-и-комментарии)) |
| Строки | Escapes `\n \t \r \\ \" \0 \u{…}` (только скобочная форма, от 1 до 6 hex-цифр, без surrogates; после `\0` не может идти цифра); raw `r"…"`, `r#"…"#`, `r##"…"##`…; многострочные `"""…"""` с удалением общего отступа; интерполяции нет (см. [Строковые литералы](../../reference/grammar/#строковые-литералы)) |
| Операторы | Фиксированная таблица, пользовательских операторов нет. От слабого к сильному: `\|>` (лев.), `\|\|` (прав.), `&&` (прав.), `== != < <= > >=` (неассоц.), `++` (прав.), `+ -` (лев.), `* / %` (лев., `%` = JS remainder), `**` (прав.), префиксные `-`, `not`, `await`, `ignore`, применение, `.field` / `M.x`. Bitwise — именованные функции `Number.bit_*` (см. [Таблица операторов](../../reference/grammar/#таблица-операторов)) |
| Bool и `if` | `Bool` = JS boolean; литералы `true`/`false` (допустимы в паттернах); `if c then a else b` — выражение, `else` обязателен, цепочки `else if`, многострочная форма по отступам (см. [Правила отступов](../../reference/grammar/#правила-отступов) и [Выражения](../../reference/grammar/#выражения)) |
| Числовые литералы | Грамматика JS `NumericLiteral`: decimal, экспонента `1e-3`, `0x`/`0o`/`0b`, разделитель `_` по правилам JS (не в начале/конце группы цифр, не двойной, не рядом с `.`/`e`/префиксом), BigInt-суффикс `n` (без дроби и экспоненты); legacy octal `010` и ведущий ноль вроде `09` запрещены. Отличие от JS: формы `.5` и `5.` запрещены — конфликт с доступом `.field` (см. [Числовые литералы](../../reference/grammar/#числовые-литералы)) |
| Тесты | Без нового синтаксиса: модуль из test root экспортирует `tests : List Test.Test`; `Test.test`, `Test.effect_test`, `Test.property`, `Test.group`, generators `Gen.*` — функции библиотеки (см. [Тесты](../../reference/stdlib/#тесты)) |
| API платформы | `Platform.args`, `Platform.env`, `stdout_line`/`stderr_line`/`stdout_write` — модуль стандартной библиотеки (см. [Модули](../../reference/stdlib/#модули)) |
| `AppError` | Тип Prelude: `type error AppError = AppError { message : String; details : Option String }`; `App.fail : String ->{; AppError} a`; runner безопасно печатает `error: <message>` и выходит с кодом `1` (см. [Средство запуска точки входа](../../reference/runtime/#средство-запуска-точки-входа)) |
| SQL table metadata | Блок `sql table Name "table" { col : sql_type [not null \| null] [primary key] [unique] [default] [generated always \| generated by default] as T [via codec] }`; объявляет тип строки `Name` и таблицу `Name$table : Sql.Table "table" { … }` из `Sql.Column`; таблицы, сгенерированные из миграций, имеют ту же форму (см. [Таблицы](../../reference/sql/#таблицы)) |
| FFI overrides | Importer пишет только `<Pkg>/Generated.fl`; собственный модуль, обычно `<Pkg>.fl`, содержит обычные `foreign`-декларации с `reviewed "<pkg>@<ver> sha256:…"` и re-export остального; `flick ffi check` сверяет fingerprint (см. [Проверенные привязки и `ffi check`](../../reference/ffi/#проверенные-привязки-и-ffi-check)) |
| Shadowing и неоднозначность | Локальные связывания затеняют всё без диагностики. Top-level определение с именем из `import … exposing (name)` — ошибка `FLICK-R009`; имена из `exposing (..)` top-level затеняет молча. Два импорта по имени с разными сущностями — `FLICK-R010` сразу; конфликт между `(..)`-импортами — `FLICK-R010` только в месте использования; два импорта под одним qualifier — `FLICK-R010`. Re-export той же сущности не конфликт. Порядок `import` ни на что не влияет (см. [Затенение и неоднозначность](../../reference/modules/#затенение-и-неоднозначность)) |
| HTTP | Без синтаксиса (см. [Граница языкового ядра](../../spec/07-errors-and-concurrency/#граница-языкового-ядра)): `Http.*` и `HttpApi.*` — библиотечные combinators; части endpoint'ов описывают codecs из `Schema a` (см. [Типизированные HTTP API](../../reference/http-api/)) |
| CLI eDSL | См. [CLI eDSL](../../spec/10-type-programming-and-edsl/#cli-edsl-флаги-аргументы-и-подкоманды). Блок `cli "name" { … }` с clauses `description`, `flag`, `arg`, `subcommand`, `handle`; elaborates в библиотечные `Cli.*` поверх `effect/cli`, доступные и напрямую (см. [CLI eDSL](../../reference/cli-dsl/) и [грамматику](../../reference/grammar/#выражения)) |
| Методы класса | Метод объявляется обычной сигнатурой `name : T` (ключевого слова `val` нет, старая запись — `FLICK-S010`). Метод по умолчанию: сигнатура и `let name … =` прямо под ней в теле класса; instance может его не определять. Сигнатура без `let` — абстрактный метод, instance обязан его определить (иначе `FLICK-C011`; см. [Экземпляры классов](../../reference/classes/#экземпляры-классов)) |
| Пакеты с модулями Flick | Пакет объявляет в своём `package.json` поле `"flick": { "iface-dir": "<dir>" }`; модуль `A.B` описан файлом `<dir>/A/B.iface`. Файлы пишет `flick build`, поле — `flick pack`, не пользователь. Установка — только pnpm (`flick install`/`add`), Flick читает установленное дерево прямых зависимостей проекта (см. [Пакеты](../../reference/project/#пакеты) и [Расположение в пакете](../../reference/iface/#расположение-в-пакете)) |
