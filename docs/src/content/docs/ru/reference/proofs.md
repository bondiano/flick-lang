---
title: "Доказательства"
source_hash: 4ede55225153d208
---

`proof` формулирует свойство определений модуля и доказывает его.
Компилятор проверяет каждое доказательство небольшим ядром и затем стирает
его: у доказательства нет значения, оно нигде не выполняется и ничего не
меняет в программе. Доказательства подключаются по желанию, и модуль без них
компилируется как прежде.

## Объявления доказательств

```flick
proof rebuilt (v : Option a)
where equal (match v with | None -> None | Some x -> Some x) v =
  match v with
  | None -> refl
  | Some y -> refl

proof same where same_type a a = refl
```

У доказательства есть имя, параметры, цель после `where` и доказательство
после `=`. Параметр — это образец с типом, и его переменные типов жёсткие:
доказательство верно для любого типа, как переменные сигнатуры. Стороны цели —
обычные выражения одного типа; доказательство — выражение из правил ниже.
`proof` — ключевое слово только в начале объявления, а `equal`, `holds`,
`same_type` и правила в других местах остаются именами.

## Цели

| Цель | Утверждает |
| - | - |
| `equal l r` | `l` и `r` одного типа — одно и то же значение |
| `holds p` | `Bool` `p` равен `true`, как `equal p true` |
| `same_type A B` | два типа — один и тот же тип |

Параметр `(h : goal)` — посылка: доказательство может использовать `h` как
доказательство `goal`, а лемме с посылками нужны их доказательства.

```flick
is_some : Option a -> Bool
let is_some v =
  match v with
  | Some _ -> true
  | None -> false

proof some_is_some (x : a) where holds (is_some (Some x)) = refl

proof lifted (x : a) (y : a) (h : equal x y)
where equal (Some x) (Some y) = cong (fun z -> Some z) h
```

## Правила

| Правило | Доказывает |
| - | - |
| `refl` | `l = r`, когда обе стороны вычисляются в одну нормальную форму |
| `symm p` | `r = l` из `p : l = r` |
| `trans m p q` | `l = r` из `p : l = m` и `q : m = r` |
| `cong (fun x -> e) p` | `e[a/x] = e[b/x]` из `p : a = b` |
| `funext (fun x -> p)` | `f = g` из `p : f x = g x` для каждого `x` |
| `match v with …` | цель в каждом случае параметра `v`, по ветке на конструктор |
| посылка `h` | её цель |
| `let h = p`, затем доказательство | это доказательство, где `h` доказывает то же, что `p` |
| лемма `name args` | цель более раннего доказательства при `args`, с доказанными посылками |

```flick
proof back (x : a) (y : a) (h : equal x y) where equal y x = symm h

proof eta (f : a -> b) where equal (fun x -> f x) f = funext (fun y -> refl)

proof through (x : a) (y : a) (z : a) (p : equal x y) (q : equal y z)
where equal x z = trans y p q
```

`match` в доказательстве разбирает параметр: каждая ветка доказывает цель,
в которой параметр заменён образцом ветки, а пропущенная ветка — это
`FLICK-T009`, как в любом `match`.

## Определения и леммы

Доказательство видит тела определений, которые называет: своего модуля,
импортированных модулей и экземпляров ядра, — и раскрывает их при
вычислении. Метод берётся из экземпляра, к которому разрешается его
ограничение.

```flick
twice : (a -> a) -> a -> a
let twice f x = f (f x)

proof unfolded (f : a -> a) (x : a) where equal (twice f x) (f (f x)) = refl

proof twice_identity (x : a)
where equal (twice identity x) x = trans (identity (identity x)) (unfolded identity x) refl

proof right_identity (v : Option a)
where equal (flat_map pure v) v =
  match v with
  | None -> refl
  | Some x -> refl
```

Доказательство, проверенное раньше, — лемма для следующих, а доказательства
модуля доказательств — леммы для импортирующих его модулей, как `M.name`.
Лемма доказывает свою цель ровно о тех определениях, которые называет:
`unfolded identity x` доказывает `twice identity x = identity (identity x)`, а
не `twice identity x = x`, поэтому `trans` добавляет шаг, раскрывающий
`identity`.

## Область

Доказательство говорит о значениях `PureTotalV1`: чистых тотальных функциях
над `Bool`, типами данных, которые не содержат сами себя, кортежами и
функциями. Утверждение, которое называет что-то другое, лежит вне области —
`FLICK-T082`, и доказательство не проверяется: рекурсия, рекурсивные типы
вроде `List`, внешний код, эффекты, примитивные операции вроде `+` над `Int`,
записи, а также метод или определение, принимающие словари.

```flick fail T082
proof add_zero (x : Int) where equal (x + 0) x = refl
```

```flick fail T082
last : List a -> Option a
let rec last xs =
  match xs with
  | [] -> None
  | [x] -> Some x
  | [_, ...rest] -> last rest

proof last_of_one (x : a) where equal (last [x]) (Some x) = refl
```

Тотальность значит, что доказательство ничего не говорит о функции, которая
падает или зацикливается: закон, доказанный для `Option`, верен для каждой
тотальной функции, а программа вправе вызывать и нетотальные.

## Отказы

| Код | Доказательство |
| - | - |
| `FLICK-T080` | ложно: стороны — разные значения |
| `FLICK-T081` | не закрывает цель; сообщение показывает, что осталось |
| `FLICK-T082` | называет что-то вне области |
| `FLICK-T083` | не является корректным сертификатом, как набор без леммы для закона |
| `FLICK-T084` | требует больше шагов, чем позволяет бюджет ядра |

```flick fail T080
proof never (x : a) where equal (Some x) None = refl
```

```flick fail T081
proof constant (f : Bool -> Bool) where equal (f true) (f false) = refl
```

Доказательство проверяется только в модуле без других ошибок, и его
диагностики относятся к модулю, в котором оно стоит.

## Законы и наборы

Класс формулирует свои законы членами `law`: закон — утверждение, которое
должен выполнять каждый экземпляр; он типизируется как метод по умолчанию и не
имеет доказательства. Компилятор стирает его.

```flick
class Wrap (f : Type -> Type) =
  wrap : a -> f a
  unwrap : f a -> a
  law round (x : a) where equal (unwrap (wrap x : f a)) x
```

`Functor`, `Applicative` и `Monad` формулируют свои законы в ядре.
Доказательство с целью `lawful C T` — набор: оно доказывает законы `C` и
каждого его суперкласса для экземпляра, к которому разрешается `C T`. Тело —
запись, которая для каждого закона называет лемму, доказанную раньше; законы
суперкласса — запись в поле этого суперкласса, как в словаре.

```flick
type Box a = Box a

instance Functor Box =
  let map f value =
    match value with
    | Box x -> Box (f x)

proof box_identity (v : Box a)
where equal (map identity v) v =
  match v with
  | Box x -> refl

proof box_composition (g : b -> c) (h : a -> b) (v : Box a)
where equal (map (fun x -> g (h x)) v) (map g (map h v)) =
  match v with
  | Box x -> refl

proof box where lawful Functor Box = {
  identity = box_identity
  composition = box_composition
}
```

Каждый закон читается при словаре и типах набора, и его лемма должна
утверждать ровно этот закон: те же параметры тех же типов и стороны закона с
методами этого экземпляра. Лемма о другой функции, типе или словаре ничего не
доказывает об экземпляре, а пропущенное поле для закона — это `FLICK-T083`.

```flick fail T083
type Box a = Box a

instance Functor Box =
  let map f value =
    match value with
    | Box x -> Box (f x)

proof box_identity (v : Box a)
where equal (map identity v) v =
  match v with
  | Box x -> refl

proof box where lawful Functor Box = { identity = box_identity }
```

Сломанный экземпляр не получает набора, потому что для его законов нет лемм:
`map`, который отбрасывает каждое значение, нарушает уже `identity`.

```flick fail T080
type Bag a = Empty | Full a

instance Functor Bag =
  let map _ _ = Empty

proof bag_identity (v : Bag a)
where equal (map identity v) v =
  match v with
  | Empty -> refl
  | Full x -> refl
```

`lawful C T from d` называет значение-словарь вместо экземпляра и лежит вне
области: набор говорит об экземпляре.

## Модули доказательств и `flick prove`

Доказательства могут стоять рядом с определениями в любом модуле или в
модулях доказательств в `project.proof-dir` (по умолчанию `proofs/`). Модули
доказательств импортируют модули проекта и друг друга, а модуль исходников не
импортирует их. `flick check`, `build`, `test` и `pack` их не читают, поэтому
доказательства библиотеки ничего не стоят её пользователям; `flick prove`
проверяет их, а редактор — открытый модуль доказательств ([cli](../cli/)).

Модуль с доказательствами проверяется заново при каждом запуске, поэтому
доказательство следует за правкой тела, которое называет, даже если сигнатура
не меняется. Ядро доказывает двенадцать законов своих экземпляров `Option` в
`packages/core/proofs/Laws/Option.fl` и подтверждает их набором
`lawful Monad Option`.

## Что обещает доказательство

Проверенное доказательство обещает, что его цель верна для определений,
которые прочитал компилятор, при каждом значении области. Ядро, его чтение
скомпилированных определений, а также компилятор и среды выполнения, которые
запускают программу, остаются доверенными;
[контракт доказательств](https://github.com/bondiano/flick-lang/blob/main/docs/formal/proofs.md)
описывает правила и границу доверия.
