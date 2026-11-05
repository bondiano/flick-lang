---
title: Синтаксис за пять минут
description: Все конструкции синтаксиса Flick, по одному короткому примеру на каждую.
---

Краткий обзор, чтобы читать код на Flick. Точные правила описаны в
[грамматике](../../reference/grammar/); здесь каждая конструкция показана
один раз.

## Значения

```flick
let count = 42              // Int
let ratio = 3.14            // Float
let huge = 10n              // BigInt
let greeting = "Hello,\n\"world\""
let path = r"C:\temp"       // сырая строка: без экранирования
let done = true             // Bool: true или false
let nothing = ()            // Unit
```

Интерполяции строк нет: строки соединяются через `++`. Пишите `0.5`, а не
`.5`: точка всегда означает доступ к полю.

## Комментарии

```flick
// строчный комментарий
/// документирующий комментарий, его читает `flick doc`
/* блочный комментарий /* может быть вложенным */ */
let x = 1
```

## Имена

- `user`, `load_user`: значения, функции, поля и переменные типов
  начинаются со строчной буквы или `_`.
- `User`, `Some`, `App.Main`: типы, конструкторы, модули, классы, сервисы и
  ошибки начинаются с заглавной буквы.
- `List.map` — имя из модуля, `user.name` — поле. Пробелов вокруг точки
  нет ни там, ни там.

## Функции

Над `let` именованной функции стоит её сигнатура. Аргументы разделяются
пробелами, без скобок и запятых:

```flick
add : Int -> Int -> Int
let add x y = x + y

let three = add 1 2

increment : Int -> Int
let increment = add 1
```

Функции каррированы: `add 1` — функция, которая ждёт `y`. Анонимная
функция записывается через `fun`:

```flick
let doubled = List.map (fun x -> x * 2) [1, 2, 3]
```

## Главное правило: скобки только группируют

`f (a, b)` — **не** вызов с двумя аргументами. `(a, b)` — кортеж, поэтому
функция получает один аргумент, и пробел перед скобкой ничего не меняет:

```text
add 1 2        // 3
add (1, 2)     // FLICK-T003: ожидался `Int`, получен `(Int, Int)`
add(1, 2)      // та же ошибка
```

Скобки нужны для группировки и для кортежей: `show (add 1 2)`.

## Отступы вместо фигурных скобок

Тело функции, ветки `match` или `if` — это строки ниже с бо́льшим отступом.
Нет ни `in`, ни `return`: значение блока — его последняя строка.

```flick
# type alias User = { name : String; age : Int }
#
describe : User -> String
let describe user =
  let years = show user.age
  user.name ++ ", " ++ years
```

Строка с бо́льшим отступом продолжает предыдущую, строка на той же колонке
начинает следующую инструкцию. Табуляция — ошибка.

## Конвейеры

`x |> f` — это `f x`. Длинная цепочка читается сверху вниз:

```flick
total : List Int -> Int
let total xs =
  xs
  |> List.filter (fun x -> x > 0)
  |> List.sum
```

## Кортежи, списки и записи

```flick
let pair = (1, "one")
let numbers = [1, 2, 3]
let range = [1..10]

type alias User = { name : String; age : Int }

let ada : User = { name = "Ada"; age = 36 }
let older = { ada with age = ada.age + 1 }
```

Записи точные: значение с лишним полем — не `User`. Поля разделяются `;`
или переводом строки.

## Типы

`type` объявляет варианты (алгебраический тип); `type alias` даёт типу
другое имя. Параметры типа идут после имени.

```flick
type Shape = Circle Float | Rect Float Float | Empty
  deriving Eq, Show

type Tree a = Leaf | Node (Tree a) a (Tree a)
```

`Option a` (`None | Some a`) и `Result e a` (`Err e | Ok a`) объявлены так
же, в Prelude.

## Сопоставление с образцом

`match` должен покрывать все значения; компилятор покажет пропущенный
случай.

```flick
# type Shape = Circle Float | Rect Float Float | Empty
#
area : Shape -> Float
let area shape =
  match shape with
  | Circle r -> 3.14 * r * r
  | Rect w h -> w * h
  | Empty -> 0.0

kind_of : Shape -> String
let kind_of shape =
  match shape with
  | Rect w h when w == h -> "square"
  | Circle _ | Empty -> "round or nothing"
  | Rect _ _ -> "rectangle"

first_or : a -> List a -> a
let first_or fallback xs =
  match xs with
  | [] -> fallback
  | [x, ..._] -> x
```

`when` добавляет условие, `|` объединяет варианты, `_` подходит к чему
угодно, а `...rest` забирает остаток списка.

## Условия

`if` — выражение, поэтому обе ветки одного типа:

```flick
sign : Int -> String
let sign n = if n < 0 then "negative" else if n == 0 then "zero" else "positive"
```

## Эффекты и ошибки

Фигурные скобки после стрелки перечисляют, что вызову нужно во время
выполнения: сервисы до `;`, ожидаемые ошибки после. Функция с такой стрелкой
вызывает другие такие функции напрямую, как чистые.

```flick
# type alias User = { name : String; age : Int }
#
type error NotFound = NotFound String

service Users =
  get : String ->{; NotFound} User

greet : String ->{Users; NotFound} String
let greet id =
  let user = Users.get id
  "Hello, " ++ user.name

greet_or_guest : String ->{Users;} String
let greet_or_guest id =
  try greet id with
  | NotFound _ -> "Hello, guest"
```

`try … with` обрабатывает `NotFound`, поэтому этой ошибки нет в строке
`greet_or_guest`. `task { … }` сохраняет вычисление, не запуская его, а
`await` запускает:

```flick
# type error NotFound = NotFound String
# service Users =
#   get : String ->{; NotFound} String
# greet : String ->{Users; NotFound} String
# let greet id = Users.get id
#
twice : String ->{Users; NotFound} (String, String)
let twice id =
  let request = task { greet id }
  let first = await request
  (first, await request)
```

`a -> b` — чистая функция, `a ->{} b` — вызов во время выполнения, которому
ничего не нужно: это разные типы. Строки подробно описаны в разделе
[Эффекты, ошибки и сервисы](../../reference/effects/).

## Модули

Файл начинается с имени модуля и того, что он открывает, затем идут импорты:

```flick
module App.Main exposing (main)

import App.Greeting exposing (greet)

main : Unit ->{; AppError} Unit
let main () =
  Platform.stdout_line (greet "Ada")
```

`import Effect.Log as Log` даёт модулю короткое имя. Встроенные модули,
например `List`, `String` и `Platform`, импортировать не нужно.

## Дальше

- [Начало работы](../../start/getting-started/): установить Flick и
  запустить программу.
- [Изучение Flick](../): путь через типы, эффекты и сервисы.
- [Ключевые слова](../../reference/keywords/): все зарезервированные слова
  с примерами.
- [Песочница](../../play/): попробовать синтаксис в браузере.
