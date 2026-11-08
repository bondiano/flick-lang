---
title: "GADT"
source_hash: 023d281fcacf0565
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

```flick fail
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

## Ограничения

- Уточнение типа доступно только в ветви `match`. В образце `let`, образце параметра и альтернативе
  or-образца оно недоступно; в этих случаях возникает `FLICK-T058`.
- Ни одна ветвь не узнаёт, что два региона или два ряда совпадают (`FLICK-T058`).
- В TypeScript уточнения нет: в файле `.d.ts` каждый конструктор — вариант любого
  экземпляра своего типа, а скрытый им тип — `unknown`.
