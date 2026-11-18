---
title: "GADT"
source_hash: 2310454d90ab9750
---

В обобщённом алгебраическом типе данных каждый конструктор сам говорит, какой
тип он строит. Тогда `match` узнаёт этот тип внутри ветви: вычислитель
возвращает ровно то, что описывает его вход, а бессмысленное выражение не
компилируется. Всё на этой странице работает на JavaScript и Go. Полные правила
описаны в разделе [Кинды и типы](../types/#gadt).

## Объявление

После `:` указывается тип результата конструктора:

```flick
type Expr a =
  | NatLit Int : Expr Int
  | BoolLit Bool : Expr Bool
  | Add (Expr Int) (Expr Int) : Expr Int
  | If (Expr Bool) (Expr a) (Expr a) : Expr a
```

`NatLit` строит `Expr Int`, `BoolLit` — `Expr Bool`, а `If` — тип своих двух
ветвей. Компилятор сверяет с этими типами каждое использование, поэтому сложение
булева значения с числом даёт `FLICK-T003` ещё до запуска:

```flick fail T003
# type Expr a =
#   | NatLit Int : Expr Int
#   | BoolLit Bool : Expr Bool
#   | Add (Expr Int) (Expr Int) : Expr Int
#   | If (Expr Bool) (Expr a) (Expr a) : Expr a
bad : Expr Int
let bad = Add (NatLit 1) (BoolLit true)
```

## Уточнение типа в ветви

При сопоставлении с конструктором внутри ветви уточняется тип `a`, поэтому
тип результата функции зависит от типа входного значения:

```flick
# type Expr a =
#   | NatLit Int : Expr Int
#   | BoolLit Bool : Expr Bool
#   | Add (Expr Int) (Expr Int) : Expr Int
#   | If (Expr Bool) (Expr a) (Expr a) : Expr a
eval : forall a. Expr a -> a
let rec eval e =
  match e with
  | NatLit n -> n
  | BoolLit b -> b
  | Add x y -> eval x + eval y
  | If c t f -> if eval c then eval t else eval f
```

В ветви `NatLit` тип `a` равен `Int`, поэтому `n` — результат типа `a`. В ветви
`BoolLit` тип `a` равен `Bool`. Редактор показывает такое значение как
`Int (= a)`. Выражение
`eval (If (BoolLit true) (Add (NatLit 1) (NatLit 2)) (NatLit 0))` равно `3`.

## Где нужна сигнатура

Уточнение типа в ветви доступно только при полной сигнатуре функции. У функции верхнего уровня она
есть всегда, а без неё `eval` даёт `FLICK-T004`. Это требование действует и тогда, когда ветви
не используют уточнения типов:

```flick
# type Expr a =
#   | NatLit Int : Expr Int
#   | BoolLit Bool : Expr Bool
#   | Add (Expr Int) (Expr Int) : Expr Int
#   | If (Expr Bool) (Expr a) (Expr a) : Expr a
is_literal : Expr a -> Bool
let is_literal e =
  match e with
  | NatLit _ -> true
  | BoolLit _ -> true
  | Add _ _ -> false
  | If _ _ _ -> false
```

Другим локальным функциям сигнатура не нужна, а той, что сопоставляет значение
GADT, нужна: без неё тип значения в `match` неизвестен, и это `FLICK-T057`.

## Тип со структурой

Ветвь может узнать и тип, составленный из частей:

```flick
type Two a =
  | One Int : Two Int
  | Both (Two x) (Two y) : Two (x, y)

eval_two : forall a. Two a -> a
let rec eval_two t =
  match t with
  | One n -> n
  | Both l r -> (eval_two l, eval_two r)
```

В ветви `Both` тип `a` — это пара `(x, y)`, поэтому ветвь и возвращает пару.
Выражение `eval_two (Both (One 1) (One 2))` равно `(1, 2)`.

## Нет ветви для невозможного

Функции, которая принимает `Expr Int`, не нужна ветвь `BoolLit`:

```flick
# type Expr a =
#   | NatLit Int : Expr Int
#   | BoolLit Bool : Expr Bool
#   | Add (Expr Int) (Expr Int) : Expr Int
#   | If (Expr Bool) (Expr a) (Expr a) : Expr a
is_nat_literal : Expr Int -> Bool
let is_nat_literal e =
  match e with
  | NatLit _ -> true
  | Add _ _ -> false
  | If _ _ _ -> false
```

Ветвь `BoolLit` здесь даёт предупреждение `FLICK-T010`: она никогда не
выполнится. Ветвь `If` обязательна, потому что `If` может построить `Expr Int`, и
без неё `match` даёт `FLICK-T009`.

## Значения с общим типом

Проверка прослеживает переменную типа через все части сопоставляемого значения:

```flick
type Tagged a =
  | IsNat : Tagged Int
  | IsBool : Tagged Bool

agree : Tagged a -> Tagged a -> Bool
let agree x y =
  match (x, y) with
  | (IsNat, IsNat) -> true
  | (IsBool, IsBool) -> true
```

Если `x` равен `IsNat`, то `a` равен `Int`, и `y` не может быть `IsBool`. Смешанную
пару никто не построит, поэтому ветвь для неё не нужна, а написанная ветвь даёт
`FLICK-T010`. При сигнатуре `Tagged a -> Tagged b` типы могут различаться, и тот же
`match` даёт `FLICK-T009`. Тип, который конструктор скрывает, не прослеживается:
`match` по `| Packed (Tagged x) (Tagged x) : Packed` требует ветвь `Packed _ _`
для смешанных значений, хотя таких не построить.

## Свидетельство равенства типов

Конструктор может требовать, чтобы два типа совпадали:

```flick
type Equ a b =
  | Refl : Equ a a

coerce : Equ a b -> a -> b
let coerce eq =
  match eq with
  | Refl -> fun x -> x
```

Сопоставление с `Refl` говорит ветви, что `a` и `b` — один тип, поэтому вход уже
является результатом, и во время выполнения ничего не происходит. Объявлять такой
тип не обязательно: он есть в модуле ядра `TypeEq` как `TypeEq a b`, `Refl` и
[`TypeEq.cast`](../types/#gadt).

## Равенство типов

Выражения строк сравниваются по смыслу во всём типе, в том числе внутри
инвариантных контейнеров. Перестановка сервисов, повтор одного сервиса или
другая расстановка скобок в объединении сохраняют тип.

```flick
service Log =
  line : String ->{} Unit

service Clock =
  now : Unit ->{} Int

same : Ref (Eff {Log, Clock} none Unit) -> Ref (Eff {Clock, Log} none Unit)
let same cell = cell
```

У ссылки по-прежнему один тип содержимого. Равенство не разрешает расширять
строку требований или ошибок вычисления, хранимого в ссылке:

```flick fail T003
# service Log =
#   line : String ->{} Unit
# service Clock =
#   now : Unit ->{} Int
widen : Ref (Eff {Log} none Unit) -> Ref (Eff {Log, Clock} none Unit)
let widen cell = cell
```

Это же различие действует в аргументах номинальных типов и полях записей. Равенство типов сохраняет идентичности сервисов и их
аргументы типов, чистые и эффектные стадии, регионы и foreign-контракты.
Разные номинальные объявления от этого не становятся взаимозаменяемыми.
См. [кинды и типы](../types/) и
[эффекты](../effects/).

При унификации в ветви GADT также используется равенство типов: другая
запись той же строки не делает ветвь невозможной. Паттерны также могут
[уточнять индексы строк сервисов, ошибок и захватов](#индексы-строк).
Идентичности регионов остаются жёсткими.

Это общие правила языка для JS и Go. Бэкенд получает проверенную программу;
выбор представления в выходном коде не задаёт другое равенство типов.

## Индексы строк

Результат конструктора может фиксировать или связывать индексы киндов
`ServiceRow`, `ErrorRow` и `CaptureRow`. Тело ветви `match` проверяется
под этими равенствами. Функции по-прежнему нужна полная сигнатура.

```flick
service Db =
  get : Int ->{} Int

type Cmd (r : ServiceRow) a =
  | Value a : Cmd none a
  | Read Int : Cmd {Db} Int

run : Cmd r a ->{r} a
let run command =
  match command with
  | Value value -> value
  | Read key -> Db.get key
```

Равенство в ветви `Read` разрешает `Db`. Оно сохраняет эффектную стадию:
даже вызов с опустевшей строкой требований остаётся эффектным вызовом.
Уточнение не делает ту же реализацию чистой функцией.

```flick fail E001
# service Db =
#   get : Int ->{} Int
# type Cmd (r : ServiceRow) =
#   | Read Int : Cmd {Db}
pure : Cmd r -> Int
let pure command =
  match command with
  | Read key -> Db.get key
```

У равенства может быть несколько остаточных строк. Здесь `s` может быть
пустой или содержать `Db`; ветвь доказывает, что любая допустимая `s`
включается в `{Db}`. Она не выбирает пустой остаток.

```flick
# service Db =
#   get : Int ->{} Int
type Packed (r : ServiceRow) =
  | Packed (Eff s none Int) : Packed {Db | s}

unpack : Packed {Db} ->{Db} Int
let unpack packed =
  match packed with
  | Packed work -> await work
```

Проверка полноты сохраняет эти равенства при проверке остальных частей
кортежа. Здесь первая часть доказывает, что `r` содержит `Db`, поэтому во
второй части не может встретиться `Empty`:

```flick
# service Db =
#   get : Int ->{} Int
type Includes (r : ServiceRow) (s : ServiceRow) =
  | Includes : Includes r {Db | r}

type RowCmd (r : ServiceRow) =
  | Empty : RowCmd none
  | WithDb Int : RowCmd {Db}

read_value : (Includes r r, RowCmd r) -> Int
let read_value pair =
  match pair with
  | (Includes, WithDb value) -> value
```

Скрытые остаточные строки разных вхождений конструктора остаются
независимыми. Части значения связывают только их общие внешние индексы.

То же рассуждение действует для строк ошибок. Локальные обработчики
снимают покрываемые ими требования до проверки внешней сигнатуры.
Равенства остаются внутри ветви, в том числе при проверках, отложенных
до конца объявления. Аргументы сервисов и ошибок остаются инвариантными:
наличие `Store String` в строке не разрешает `Store Int`.

Индексы захватов могут связывать допустимые захваты значений:

```flick
type Share (c : CaptureRow) (d : CaptureRow) =
  | Share : Share c c

share : Share c d -> (Unit -> Int)^c -> (Unit -> Int)^d
let share proof callback =
  match proof with
  | Share -> callback
```

Это не отождествляет разные регионы. Параметры кинда `Region` и параметры
строк записей по-прежнему передаются переменными; паттерны не выводят
новую идентичность региона. Уточнение также сохраняет действующие правила
для обработчиков, foreign-вызовов и изменяемых ссылок.

Это общие правила для JS и Go. Остальные ограничения уточняющих ветвей
— [равенство типов](#равенство-типов) и [ограничения](#ограничения) ниже.

## Ограничения

- Уточнение типа доступно только в ветви `match`. В образце `let`, образце параметра и альтернативе
  or-образца оно недоступно; в этих случаях возникает `FLICK-T058`.
- Паттерны не отождествляют разные регионы (`FLICK-T058`).
  [Индексы строк сервисов, ошибок и захватов](#индексы-строк) могут уточняться.
- В TypeScript уточнения нет: в файле `.d.ts` каждый конструктор — вариант любого
  экземпляра своего типа, а скрытый им тип — `unknown`.
- Lean доказывает правила индексов строк на примерах
  (`formal/Formal/GadtRowExamples.lean`): принадлежность из поглощения,
  включение для любого остатка, исключение вычитанием, перенос равенства
  захватов и то, что выбор пустого остатка потерял бы решение. Общего
  доказательства корректности параметризованных индексов строк нет, и примеры
  не подтверждают алгоритм анализатора, который тесты сравнивают с эталонной
  моделью (`docs/formal/gadt-rows.md`).
