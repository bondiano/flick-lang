---
title: "Свой типизированный DSL"
source_hash: 9b6b28706ad72a37
---

DSL должен превращать предметный язык в обычное проверяемое значение, а не в
строку в обход компилятора. Построим `validate { positive 42 }` с результатом
`Result ValidationError Int`. В [рабочем примере](https://github.com/bondiano/flick-lang/tree/main/examples/typed-dsl)
есть Rust-плагин, библиотека Flick и приложение-потребитель.

## Начните с результата

```flick
module PositiveDsl exposing (ValidationError(..), positive)
type ValidationError = NotPositive deriving Show
positive : Int -> Result ValidationError Int
let positive n = if n > 0 then Ok n else Err NotPositive
```

`Err NotPositive` — предметный результат, не ошибка компиляции. Ноль — допустимый
Int; программа сама обрабатывает отказ. Строка вместо Int должна быть отклонена
компилятором.

## Соберите и импортируйте плагин

Из корня репозитория:

```sh
pnpm install --frozen-lockfile
pnpm -C runtime build
pnpm -C packages/effect-ts build
cargo build -p flick-example-typed-dsl --features guest --target wasm32-wasip2 --release
cargo build -p flick --features wasm-plugins
node examples/typed-dsl/check.mjs
```

Скрипт копирует компонент в библиотеку, проверяет принятые и отклонённые программы,
запускает оба предметных исхода и восстанавливает исходник даже при сбое.
Компилятор должен поддерживать WASM-плагины.

В `flick.toml` библиотеки указано `plugin = "validation.wasm"` в `[package]`,
а приложение зависит от `validation = { path = "../library" }`. Импортируем тип
результата и голову DSL:

```flick
module Main exposing (answer)
import PositiveDsl exposing (ValidationError, plugin validate)
answer : Result ValidationError Int
let answer = validate { positive 42 }
```

Markdown-doctest доверяет импортированным пакетам. Именно проектная проверка выше
доказывает, что внешний плагин действительно раскрывается и выполняется.

## Прочитайте синтаксис, верните код

Откройте `plugin/src/lib.rs`:

1. `collect` возвращает пустое состояние: общей схемы у блоков нет.
2. `expand` находит первый токен тела; вместо `positive` выдаёт
   `validation/V001` на его span.
3. `Compiler::parse_expr` разбирает выражение после слова как hole, а не как
   текст, повторно разбираемый плагином. Лишние токены отклоняются.
4. `flick!(b, PositiveDsl.positive #hole)` строит arena. Сгенерированное имя
   квалифицировано, а пользовательское выражение сохраняет происхождение.
5. Flick разрешает имена и проверяет arena. Поэтому `positive "42"` даёт
   `FLICK-T003`, а `positive 0` компилируется и возвращает `Err NotPositive`.

Плагин — WASM-компонент версионированного SDK, не shell-команда. Примеру не нужны
разрешения на файлы, сеть или переменные окружения. Протокол, ограничения ресурсов
и внешние входы описаны в разделе [Плагины](../../reference/plugins/).

## Подсказки редактора используют тот же синтаксис

`complete` предлагает `positive` с сигнатурой результата; запрос member-completion
оставляется компилятору. `hover` находит span ключевого слова и объясняет отказ.
Spans — диапазоны UTF-8 байтов относительно блока, не файла; host переносит их
в пользовательский исходник. Эти hooks не заменяют проверку типов. Минимальный
пример не реализует подсветку ключевых слов и собственный formatter.

## SQL по миграциям: цикл исправления LLM

Более крупный DSL публикует схему для проверки других блоков. SQL-плагин Flick
получает `People` из миграции без подключения к серверу. Пусть LLM выдаёт:

```flick fail std/Q001
# import Sql
sql migration 1 "people" {
  create table people (id bigint primary key, name text not null);
}
let names = sql { from p in People select { name = p.nickname } }
```

Сохраните модуль и выполните `flick check`. Неизвестная колонка даёт `std/Q001`:
в таблице из миграции нет `nickname`. Верните генератору диагностику и схему.
Исправленный блок:

```flick
# import Sql
sql migration 1 "people" {
  create table people (id bigint primary key, name text not null);
}
names : Sql.Template Sql.Pg {} { name : String }
let names = sql { from p in People select { name = p.name } }
```

Повторите `flick check`: явная сигнатура проверяет PostgreSQL-шаблон и тип строки.
Этот цикл воспроизводим без LLM-сервиса в CI. Диагностика — обратная связь, а не
доказательство правильности сгенерированной программы.

## Границы проверки

- Компилятор проверяет сгенерированные выражения, входные типы и объявленный
  результат. Плагин отвечает за грамматику и предметные статические правила.
- SQL проверяется по исходной схеме в поддерживаемом подмножестве. Это не
  доказывает, что живая БД применила миграции, права доступа верны, запрос быстр
  или выбранные данные соответствуют бизнес-намерению.
- Предметный `Result` остаётся обязанностью вызывающего кода. Компиляция и один
  пример не доказывают корректность произвольного плагина. Независимо тестируйте
  отклоняемые программы и результаты выполнения.

## Близкие подходы

| Подход | Синтаксис и источник типов | Диагностика и инструменты |
| - | - | - |
| [Kotlin DSL](https://kotlinlang.org/docs/type-safe-builders.html) | Обычные builders Kotlin и лямбды с receiver; типы из API библиотеки. | Стандартный compiler/IDE; `@DslMarker` ограничивает области receiver. |
| [Rust proc macros](https://doc.rust-lang.org/reference/procedural-macros.html) | Потоки токенов раскрываются в проверяемый Rust-код. | Автор управляет spans и ошибками; раскрытие негигиенично. |
| [F# type providers](https://learn.microsoft.com/en-us/dotnet/fsharp/tutorials/type-providers/) | Типы и члены предоставляются по внешним данным или схеме. | Инструменты видят предоставленные типы; гарантии зависят от входов и реализации provider. |
| Плагины Flick | Импортированные головы владеют синтаксисом и возвращают core arena с типизированными holes. | Hooks диагностики, completion и hover используют spans блока; сгенерированный код проверяется обычно. |

Если предметную область ясно выражают функции библиотеки, начните с них. Плагин
оправдан, когда синтаксис и подсказки по схеме окупают parser, editor hooks и
регрессии. Далее: [SQL](../../reference/sql/) и [тестирование](../../contributing/testing/).
