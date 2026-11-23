---
title: Зачем Flick
description: Какие гарантии даёт Flick, каковы их ограничения и кому язык пока не подходит.
source_hash: acde3e41cff28fa3
---

Flick — один язык для всего приложения: браузера, сервера и командной строки. Его
главная идея умещается в одной сигнатуре:

```flick
# opaque type alias UserId = String
# type alias User = { id : UserId; name : String }
# type alias UserView = { id : UserId; title : String }
# to_view : User -> UserView
# let to_view user = { id = user.id; title = String.trim user.name }
#
type error NotFound = NotFound UserId

service Users =
  get : UserId ->{; NotFound} User

service Log =
  info : String ->{} Unit

load_view : UserId ->{Users, Log; NotFound} UserView
let load_view id =
  let user = Users.get id
  let view = to_view user
  Log.info "Loaded \{view.title}"
  view
```

Прочитайте `load_view` слева направо. Функция принимает `UserId`, использует
сервисы `Users` и `Log`, может завершиться ошибкой `NotFound`, а при успехе
возвращает `UserView`. Компилятор проверяет, что реализация функции соответствует
её сигнатуре. Если функция вызовет сервис, не указанный в сигнатуре, программа
не скомпилируется.

## За чем следит компилятор

- **Чистота функций проверяется.** `a -> b` — чистая функция. Эффектный вызов в
  ней — `FLICK-E001`.
- **Сервисы и ошибки названы.** Используемые сервисы и возможные ошибки должны
  быть указаны в сигнатуре. Иначе компилятор сообщает `FLICK-E002` и предлагает
  её дополнить.
- **Ошибки обрабатываются.** `try … with` убирает `NotFound`, только если
  обработчики покрывают все её значения. Иначе ошибка остаётся в сигнатуре
  вызывающей функции.
- **У каждой функции верхнего уровня есть сигнатура.** Функция без неё —
  `FLICK-T004`, а сообщение предлагает выведенный тип. Локальная функция может
  обойтись без неё: она доступна только в своей области видимости.
- **Каждый `match` обрабатывает все варианты.** Необработанный вариант — `FLICK-T009`.
- **Ресурсы нельзя использовать после закрытия области.** При завершении
  `scope { … }` освобождаются принадлежащие ему ресурсы. Компилятор не
  позволяет сохранить доступ к ним за пределами этой области.

Замыкание ниже попыталось бы прочитать файл после его закрытия при выходе из
области владения. Компилятор отвергает его с `FLICK-G002`.

```flick fail G002
# import File exposing (Path, FileSystem, OpenError, ReadError)
#
read_later : Path ->{Async, FileSystem; OpenError} (Unit ->{Async, FileSystem; ReadError} String)
let read_later path =
  scope {
    let file = File.open_scoped path
    fun () -> File.read_all file
  }
```

Полные правила — в разделах [Эффекты и ошибки](../../reference/effects/),
[Сервисы, слои и хост](../../reference/services/) и [Регионы и захваты](../../reference/regions/).

## Ограничения

- **Дефекты остаются вне сигнатуры.** Чистая функция всё равно может упасть с
  дефектом или не завершиться. Эффектную функцию ещё могут прервать.
- **`foreign` принимается на веру.** Компилятор верит сигнатуре внешней привязки.
  `flick ffi check` сверяет её только с `.d.ts` пакета.
- **Пустая строка не изолирует.** Стандартная библиотека называет консоль,
  часы, случайные числа, окружение, аргументы и дочерние процессы сервисами
  `Host`, но привязка `foreign` и `Host.raw_*` обращаются к хосту, что бы ни
  говорила строка (см.
  [чего не обещает пустая строка](../../reference/services/#чего-не-обещает-пустая-строка)).
- **Доказательства покрывают ядро, а не весь компилятор.** Базовое исчисление
  доказано на Lean при оговорённых гипотезах, а известные отличия от проверки
  типов записаны. См. [`formal/`](https://github.com/bondiano/flick-lang/tree/main/formal).

## Эффектный код читается как обычный

Чтобы писать на Flick, не нужно знать, что такое монада. Вызов сервиса — это
прямой вызов, `let` связывает его результат, а строки выполняются сверху вниз.
В `load_view` выше нет `yield`, `flatMap` и `pipe`.

Значения и типизированные ошибки работают вместе. Здесь `?` извлекает успешное
значение `Result` или поднимает его ошибку, а `catch` превращает эту ошибку в
сообщение. Сигнатура `sum` называет возможную ошибку; `sum_text` обрабатывает её,
поэтому в строке ошибок этой функции ничего нет.

```flick
type error InvalidNumber = InvalidNumber String

parse_number : String -> Result InvalidNumber Float
let parse_number text =
  match String.to_number text with
  | Some number -> Ok number
  | None -> Err (InvalidNumber text)

sum : String -> String ->{; InvalidNumber} Float
let sum left right = (parse_number left)? + (parse_number right)?

sum_text : String -> String ->{} String
let sum_text left right =
  show (sum left right) catch
  | InvalidNumber text -> "Not a number: \{text}"
```

Используйте `result { … }`, если вызывающая функция должна получить значение
`Ok` или `Err` вместо поднятой ошибки. Для `Option` оператор `orelse`
вычисляет запасное значение только при `None`.

Параллельность задана явно. `task` создаёт вычисление, не запуская его;
`parallel` выполняет оба вычисления параллельно и возвращает их результаты.
Требования к сервисам и возможные ошибки по-прежнему видны в сигнатуре функции.

```flick
# opaque type alias UserId = String
# type alias UserView = { id : UserId; title : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} UserView
# service Log =
#   info : String ->{} Unit
# load_view : UserId ->{Users, Log; NotFound} UserView
# let load_view id =
#   let view = Users.get id
#   Log.info "Loaded \{view.title}"
#   view
#
load_two : UserId -> UserId ->{Users, Log; NotFound} (UserView, UserView)
let load_two first_id second_id =
  let first = task { load_view first_id }
  let second = task { load_view second_id }
  parallel first second
```

Сервисы позволяют подменять зависимости, не меняя функцию. `greeting` читает
часы обычным вызовом; `provide` задаёт фиксированное время для
детерминированной проверки. Требование к сервису удовлетворено внутри блока,
поэтому у `check_greeting` пустая строка сервисов.

```flick
service Clock =
  hour : Unit ->{} Int

greeting : Unit ->{Clock} String
let greeting () =
  if Clock.hour () < 12 then "Good morning" else "Good afternoon"

check_greeting : Unit ->{} Bool
let check_greeting () =
  let clock = Layer.succeed Clock { hour = fun () -> 9 }
  provide clock { greeting () == "Good morning" }
```

Локальные изменения не требуют рантайма. Эта функция использует `let mut` и
`while`, но остаётся чистой: она меняет только собственные переменные.

```flick
digits : Int -> Int
let digits number =
  let mut left = number
  let mut count = 1
  while left >= 10 do {
    left = left / 10
    count = count + 1
  }
  count
```

Циклы поддерживают `break`, `continue` и метки для вложенных циклов.
Та же структура блоков управляет ресурсами: `scope { … }` задаёт их время
жизни, а `defer { … }` регистрирует очистку при завершении этой области.
См. [эффекты и ошибки](../../reference/effects/),
[сервисы](../../reference/services/) и [ресурсы](../../reference/resources/).

## Один язык на всё приложение

Одни и те же модули собираются для двух целей. JavaScript даёт ES-модули для
браузера и Node: представления `ui` рисует React, а эффекты работают на
собственном небольшом рантайме Flick. Потоки, командная строка и часть HTTP и
RPC пока работают на [Effect](https://effect.website/) через пакет-адаптер (см.
[рантайм](../../reference/runtime/)). Go даёт один исполняемый файл, которому не
нужен Node. `just language` сравнивает поведение обеих целей в CI.

Чистый код пишется один раз и используется в обеих. В
[примере Conduit](https://github.com/bondiano/flick-lang/tree/main/examples/conduit)
Go-сервер на PostgreSQL и браузерный клиент зависят от одного пакета с контрактом
API. Несуществующий маршрут — ошибка компиляции с обеих сторон, а блоки `sql`
сверяются с миграциями, а не с живой базой.

Есть ограничения. `ui` работает только на JavaScript, а часть стандартной
библиотеки пока не компилируется в Go. Компилятор сообщает об этом кодом
`FLICK-P012` при сборке, а не во время работы.

## Библиотеки платформы вместо переписывания

Flick не нужна своя копия каждой библиотеки. Объявление `foreign` привязывает
функцию JavaScript к сигнатуре Flick, а `flick ffi import <package>` пишет
большинство таких объявлений по `.d.ts` пакета и даёт отчёт по каждому экспорту.
В обратную сторону `flick build --declarations` пишет объявления TypeScript для
модулей Flick. В Go привязки пока пишутся вручную. См.
[Внешние функции](../../reference/ffi/).

## Код, который писали не вы

Код мог написать коллега или модель; он также мог быть взят из библиотеки.
Сигнатура позволяет узнать зависимости функции и возможные ошибки до чтения её
реализации. Нарушения контракта выявляются при компиляции. Каждое сообщение
имеет код: `flick explain <code>` объясняет его, а `--diagnostics json` выводит
диагностические события для инструментов, по одному на строку.

Исходник примерно такой же длины, как то же приложение на Go и React.
[`tools/tokens`](https://github.com/bondiano/flick-lang/blob/main/tools/tokens/index.mjs)
сравнивает число токенов в `examples/conduit` и эталонном приложении RealWorld:
`node tools/tokens/index.mjs` повторяет подсчёт. По токенизатору GPT
`o200k_base` они примерно равны: 29 979 токенов против 29 778 на 18.11.2026,
отношение 1,01. По токенизатору Claude отношение было 0,93 на 05.11.2026;
инструмент считает и его, если задан `ANTHROPIC_API_KEY`. Это одно
приложение, поэтому считайте цифры грубой проверкой.

## Кому Flick пока не подходит

- **Проектам, которым нужен стабильный язык.** Версия Flick — 0.1, и до 1.0
  минорный релиз может сломать программы.
- **Командам, которым нужна большая экосистема.** Помимо стандартной библиотеки
  пакетов на Flick мало.
- **Страницам, которые должны весить мало.** UI-программа несёт React и
  рантайм Flick. По замеру `just bundle-size` на 18.11.2026 отслеживаемые в
  репозитории UI-программы весили 80–92 KB в gzip, а с адаптером Effect —
  116–128 KB.
- **Другим платформам.** Целей для мобильных устройств и десктопа нет. Цель
  WebAssembly запланирована, но в `main` её ещё нет.
- **Командам, которые уже продуктивно работают с TypeScript и Effect.** В
  JavaScript Flick по умолчанию работает на своём рантайме, а через адаптер
  (`[target.js] runtime = "effect"`) — на том же Effect, и тогда переход на него — выбор
  исходного языка, а не среды выполнения.
  [Покрытие Effect](../../reference/runtime/#покрытие-effect) перечисляет, до чего из
  Effect дотягивается Flick.

## Дальше

В разделе [Начало работы](../getting-started/) описаны установка компилятора и
запуск первой программы. [Справочник](../../reference/types/) описывает язык целиком.
