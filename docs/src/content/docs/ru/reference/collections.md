---
title: "Классы типов для коллекций"
source_hash: 8ac4cec85bbde4cf
---

Flick разделяет общие операции над коллекциями на два уровня, полностью
сохраняя квалифицированные API модулей (`List.map`, `Chunk.get`, `HashMap.get`,
`HashSet.member`, `Trie.get`, `Bytes.get`, `Seq.take`, `NonEmpty.head`):

- **Классы из `Prelude`** ([классы](../classes/#классы)) покрывают совместимое
  отображение (`Functor`), левую свёртку (`Foldable`), последовательный
  эффектный обход (`Traversable`), аппликативное и монадическое связывание
  (`Applicative`, `Monad`, `Alternative`), закрытый обход в `for` (`Iterable`)
  и ассоциативное объединение (`Semigroup`, `Monoid`).
- **Модуль ядра `Collections`** задаёт пять небольших классов возможностей,
  законы которых разделяют минимум две коллекции: `ToSeq`, `Sized`, `Indexed`,
  `Keyed` и `Member`.

Каждый класс возможностей использует функциональные зависимости на конкретном
типе коллекции `c : Type`, поэтому полиморфные коллекции (`List a`, `Chunk a`,
`HashMap w k v`), словари с фиксированным ключом (`Trie v`) и мономорфные
компактные коллекции (`Bytes` с элементом `Int`) участвуют в иерархии
единообразно. Обобщённые потребители запрашивают только нужные им возможности.

## Иерархия классов и суперклассы

```text
Prelude (над f : Type -> Type и значениями a : Type)
  Functor f
    ├── Traversable f
    └── Applicative f
          ├── Monad f
          └── Alternative f
  Foldable f
  Iterable f           (закрытый: List, Seq — обслуживает нативный `for`)
  Semigroup a => Monoid a

Collections (модуль ядра, открытые классы возможностей над c : Type)
  ToSeq c a | c -> a                  перезапускаемый ленивый обход `Seq` (включая `Seq`)
  Sized c                             конечное неотрицательное число элементов (без `Seq`)
    ├── Indexed c a | c -> a          индексный доступ 0 .. size - 1
    ├── Keyed c k v | c -> k v        поиск по ключу по свидетельству самой коллекции
    └── Member c a | c -> a           конечная проверка принадлежности по эквивалентности коллекции
```

### Почему границы проведены именно так

- **`ToSeq` и закрытый `Iterable`.** Класс `Iterable` закрыт (`FLICK-C012`),
  поскольку `for` компилируется напрямую в нативный обход `List` и `Seq` без
  вызова `iter`. Класс `ToSeq` открыт и переводит любую коллекцию в
  перезапускаемую последовательность `Seq a` с сохранением захватов; сама `Seq`
  удовлетворяет `Iterable`, поэтому `for x <- Collections.to_seq coll do ...`
  работает для любой коллекции с `ToSeq`.
- **`Sized` исключает `Seq`.** Последовательность `Seq` может быть бесконечной
  (`Seq.from`, `Seq.iterate`, `Seq.repeat`, `Seq.cycle`). Отсутствие `Sized` (и
  его подклассов `Indexed`, `Keyed` и `Member`) у `Seq` отвергает операции,
  требующие конечности — например `Collections.size`, `Collections.to_list` или
  `Collections.member` — на этапе проверки типов.
- **`Functor` для словарей — только при сохранении ключей и свидетельства.**
  `HashMap w k`, `Trie` и `OrderedMap k` реализуют `Functor`, `Foldable` и
  `Traversable` над значениями, сохраняя ключи, идентичность свидетельства `w`
  и порядок обхода. У `HashSet w` (и `OrderedSet`) намеренно нет `Functor` и
  `Traversable`: преобразование элементов множества может менять равенство и
  склеивать элементы, а также требует явного целевого свидетельства
  (`HashSet.map`).
- **`Semigroup` и `Monoid`.** Непустые коллекции (`NonEmpty`, `NonEmptyChunk`)
  реализуют `Semigroup` без `Monoid`. Хеш-коллекции со свидетельством
  (`HashMap w k v`, `HashSet w a`) реализуют `Semigroup` для любого
  свидетельства `w` (объединяя значения с общим свидетельством), а `Monoid` —
  только для `EqHash.Canonical` (где `Hash` задаёт каноническое пустое
  свидетельство).

## Сигнатуры, реализации по умолчанию и законы

### `Collections.ToSeq c a | c -> a`

```flick
# import Collections
#
check_to_seq : Collections.ToSeq c a => c^d -> (Seq a)^d
let check_to_seq xs = Collections.to_seq xs
```

| Член | Сигнатура | По умолчанию |
| - | - | - |
| `to_seq` | `forall d. c^d -> (Seq a)^d` | абстрактный |
| `first` | `forall d. c^d -> Option a` | `Seq.first (to_seq xs)` |

Суперклассы: нет.

Законы:

- `first xs == Seq.first (to_seq xs)`.
- **Перезапускаемость:** повторный обход `to_seq xs` даёт те же элементы в том
  же порядке и не изменяет `xs`.
- **Ленивость префикса:** `Seq.take n (to_seq xs)` читает не более `n`
  элементов и завершается на бесконечных `Seq`.
- **Сохранение захватов:** скрытые захваты `d` коллекции `xs` сохраняются на
  возвращаемой `(Seq a)^d`, поэтому последовательность из области `scope` не
  может покинуть её пределы.
- **Согласованность порядка с `Foldable`:** если у `f` есть и `Foldable f`, и
  `ToSeq (f a) a`, то `fold step init xs == Seq.fold step init (to_seq xs)`;
  для словарей, отображающих значения (`HashMap w k`, `Trie`, `OrderedMap k`),
  выполняется
  `fold step init m == Seq.fold (fun acc (_, v) -> step acc v) init (to_seq m)`.

### `Collections.Sized c`

| Член | Сигнатура | По умолчанию |
| - | - | - |
| `size` | `c -> Int` | абстрактный |
| `is_empty` | `c -> Bool` | `size xs == 0` |

Суперклассы: нет.

Законы:

- `size xs >= 0`.
- `is_empty xs == (size xs == 0)`.
- При наличии `ToSeq c a`: `to_seq xs` завершается и даёт ровно `size xs`
  элементов.
- При наличии `Monoid c`: `size empty == 0` и `is_empty empty == true`.

### `Sized c => Collections.Indexed c a | c -> a`

| Член | Сигнатура | По умолчанию |
| - | - | - |
| `at` | `Int -> c -> Option a` | абстрактный |
| `last` | `c -> Option a` | `at (size xs - 1) xs` |

Суперкласс: `Sized c` (даёт `size` и `is_empty`, исключает `Seq`).

Законы:

- **Границы:** `Option.is_some (at i xs) == (i >= 0 && i < size xs)`.
- **Вне границ:** `at (-1) xs == None` и `at (size xs) xs == None`.
- **Последний элемент:** `last xs == at (size xs - 1) xs`.
- **Согласованность порядка с `ToSeq`:** для каждой пары `(i, x)` из
  `Seq.enumerate (to_seq xs)` верно `at i xs == Some x`, а `at 0 xs == first xs`.

### `Sized c => Collections.Keyed c k v | c -> k v`

| Член | Сигнатура | По умолчанию |
| - | - | - |
| `lookup` | `k -> c -> Option v` | абстрактный |
| `has_key` | `k -> c -> Bool` | `Option.is_some (lookup key m)` |

Суперкласс: `Sized c`.

Законы:

- `has_key key m == Option.is_some (lookup key m)`.
- **Пустой словарь:** если `is_empty m`, то `lookup key m == None` и
  `has_key key m == false` для любого `key`.
- **Согласованность со свидетельством и `ToSeq`:** при наличии `ToSeq c (k, v)`
  каждая пара `(k, v)` из `to_seq m` удовлетворяет `lookup k m == Some v`, а
  `to_seq m` выдаёт `size m` уникальных ключей по эквивалентности `m`.
- **Согласованность с `Functor`:** для словарей, чей `Functor` сохраняет ключи
  и свидетельство, `lookup k (map f m) == Option.map f (lookup k m)` и
  `size (map f m) == size m`.

### `Sized c => Collections.Member c a | c -> a`

| Член | Сигнатура | По умолчанию |
| - | - | - |
| `member` | `a -> c -> Bool` | сигнатура по умолчанию `(ToSeq c a, Eq a) =>`: `Seq.any (fun y -> eq value y) (to_seq xs)` |

Суперкласс: `Sized c` (исключает потенциально бесконечный `Seq`).

Экземпляры со своим свидетельством равенства или порядка (`HashSet w a`,
`OrderedSet a`) переопределяют `member` напрямую и не требуют
канонического `Eq a`. Конечные последовательности с `ToSeq` и `Eq a` могут
либо дать собственную быструю проверку, либо унаследовать реализацию по
умолчанию с ранним выходом.

Законы:

- **Пустая коллекция:** если `is_empty xs`, то `member value xs == false`.
- **Согласованность с `ToSeq`:** при наличии `ToSeq c a` значение
  `member value xs` истинно тогда и только тогда, когда `to_seq xs` содержит
  элемент, эквивалентный `value` по эквивалентности `xs`.

### Алгебраические законы `Prelude`

- **`Functor`:** `map identity v == v` и
  `map (fun x -> g (h x)) v == map g (map h v)` (записаны как `law` в
  `Prelude`).
- **`Foldable`:** `fold_map f xs == fold (fun acc x -> combine acc (f x)) empty xs`,
  свёртка в порядке обхода.
- **`Traversable`:** `traverse (fun x -> x) xs` сохраняет `xs` без эффектов;
  эффекты выполняются последовательно в порядке обхода и останавливаются на
  первой типизированной ошибке (`Fail e`).
- **`Semigroup` / `Monoid`:** `combine (combine x y) z == combine x (combine y z)`,
  `combine empty x == x` и `combine x empty == x` (записаны как `law` в
  `Prelude`).

## Матрица классов и экземпляров

Поставляемые коллекции реализованы в `core` и `std`.

| Коллекция | Элемент / запись (`ToSeq`) | `Functor` | `Foldable` | `Traversable` | `Semigroup` | `Monoid` | `Iterable` | `ToSeq` | `Sized` | `Indexed` | `Keyed` | `Member` |
| - | - | - | - | - | - | - | - | - | - | - | - | - |
| `List a` | `a` | Да | Да | Да | Да | Да | Да | Да | Да | Да | Нет | `Eq a =>` |
| `Seq a` | `a` | Да | Да (конеч.) | Нет | Да | Да | Да | Да | **Нет** | Нет | Нет | Нет |
| `NonEmpty a` | `a` | Да | Да | Да | Да | Нет | Нет | Да | Да | Да | Нет | `Eq a =>` |
| `Chunk a` | `a` | Да | Да | Да | Да | Да | Нет | Да | Да | Да | Нет | `Eq a =>` |
| `NonEmptyChunk a` | `a` | Да | Да | Да | Да | Нет | Нет | Да | Да | Да | Нет | `Eq a =>` |
| `HashMap w k v` | `(k, v)` | `HashMap w k` | `HashMap w k` | `HashMap w k` | Да | `Hash k =>` (`Canonical`) | Нет | Да | Да | Нет | `(k, v)` | Нет |
| `HashSet w a` | `a` | **Нет** | `HashSet w` | Нет | Да | `Hash a =>` (`Canonical`) | Нет | Да | Да | Нет | Нет | Да (свидетельство) |
| `Trie v` | `(String, v)` | Да | Да | Да | Да | Да | Нет | Да | Да | Нет | `(String, v)` | Нет |
| `Bytes` | `Int` (`0..255`) | Нет | Нет | Нет | Да | Да | Нет | Да | Да | Да | Нет | Да (`Int`) |
| `PriorityQueue p a` | `(p, a)` | Нет | Нет | Нет | `Ord p =>` | `Ord p =>` | Нет | `Ord p =>` | Да | Нет | Нет | Нет |
| `OrderedMap k v` | `(k, v)` | `OrderedMap k` | `OrderedMap k` | `OrderedMap k` | Нет | Нет | Нет | Да | Да | Нет | `Ord k =>` | Нет |
| `OrderedSet a` | `a` | **Нет** | Да | Нет | Нет | Нет | Нет | Да | Да | Нет | Нет | `Ord a =>` |

`List`, `Seq`, `NonEmpty` и `Chunk` также реализуют `Applicative` и `Monad`
(а `List`, `Seq` и `Chunk` — `Alternative`).

## Порядок обхода, сложность и эффекты

| Коллекция | Порядок обхода | `to_seq` + префикс из `k` элементов | `size` | `at` / `lookup` / `member` | `combine` | `map` / `fold` / `traverse` |
| - | - | - | - | - | - | - |
| `List a` | по индексу `0 .. n-1` | `O(1) + O(k)` | `O(1)` | `at` `O(1)`, `member` `O(n)` | `O(n + m)` | `O(n)` |
| `Seq a` | порядок генератора (ленивый) | `O(1) + O(k)` | — | — | `O(1)` лениво | `map` `O(1)` лениво, `fold` `O(n)` |
| `NonEmpty a` | по индексу `0 .. n-1` | `O(1) + O(k)` | `O(1)` | `at` `O(1)`, `member` `O(n)` | `O(n + m)` | `O(n)` |
| `Chunk a` / `NonEmptyChunk a` | по индексу `0 .. n-1` | `O(1) + O(k log n)` | `O(1)` | `at` `O(log n)`, `member` `O(n)` | `O(1)` / `O(log n)` | `O(n)` |
| `HashMap w k v` | 5-битный HAMT от младших бит хеша, порядок вставки при коллизиях | `O(1) + O(k)` | `O(1)` | `lookup`/`has_key` `O(1)` в сред. | `O(m)`, правый выигрывает | `O(n)` |
| `HashSet w a` | 5-битный HAMT от младших бит хеша, порядок вставки при коллизиях | `O(1) + O(k)` | `O(1)` | `member` `O(1)` в сред. | `O(m)` | `fold` `O(n)` |
| `Trie v` | возрастающий порядок UTF-16 ключей | `O(1) + O(k * L)` | `O(1)` | `lookup` `O(L)` | `O(m * L)`, правый выигрывает | `O(n)` / `O(n * L)` |
| `Bytes` | по индексу байта `0 .. n-1` | `O(1) + O(k)` | `O(1)` | `at` `O(1)`, `member` `O(n)` (`O(1)` вне `0..255`) | `O(n + m)` | — |
| `PriorityQueue p a` | возрастающий `Ord p`, порядок вставки при равных приоритетах | `O(1) + O(k log n)` | `O(1)` | — | `O(m + log n)`, левый первым | — |
| `OrderedMap k v` | возрастающий `Ord k` | `O(log n) + O(k)` | `O(1)` | `lookup` `O(log n)` | — | `O(n)` |
| `OrderedSet a` | возрастающий `Ord a` | `O(log n) + O(k)` | `O(1)` | `member` `O(log n)` | — | `fold` `O(n)` |

`PriorityQueue` выполняет `peek` за `O(1)`, а `push` и `pop` — за `O(log n)` в
худшем случае, в том числе на старых версиях. `combine q1 q2` перенумеровывает
`m` пар `q2` после пар `q1`, поэтому при равных приоритетах первыми выходят
пары `q1`. Две очереди равны по `Eq`, когда выдают одни и те же пары в одном
порядке.

`OrderedMap` и `OrderedSet` — деревья, сбалансированные по весу: `insert`,
`remove`, `min` и `max` выполняются за `O(log n)`, в том числе на старых
версиях, которые обновление оставляет как были. Ключи эквивалентны, когда
`compare` даёт `EQ`, и более поздняя запись заменяет раннюю вместе с ключом.
`range { minimum; maximum }` выдаёт записи с ключами от `minimum` до `maximum`
включительно в порядке возрастания: до `minimum` он доходит за `O(log n)`, не
читая ключи перед ним, поэтому `m` записей занимают `O(log n + m)`, а диапазон,
у которого `minimum` идёт после `maximum`, пуст. `valid` проверяет
представление для тестов и отладки.

### Эффекты, короткое замыкание и захваты

- Чистые операции (`to_seq`, `first`, `size`, `is_empty`, `at`, `last`,
  `lookup`, `has_key`, `member`, `map`, `fold`, `combine`, `empty`) не
  используют эффектов и не создают файберов.
- Эффектные обходы (`Traversable.traverse`, `Collections.each_effect`,
  `Collections.fold_effect`) выполняются последовательно в порядке обхода,
  сохраняют строки сервисов и ошибок `{r; e}` и немедленно останавливаются на
  первой типизированной ошибке, не посещая оставшиеся элементы.
- Поисковые операции (`Collections.find`, `Collections.any`, `Collections.all`,
  `Collections.all_in` и `Member.member` по умолчанию) останавливаются на
  первом решающем элементе и работают на ленивых префиксах.
- Замыкания области (`(a ->{r; e} b)^c`, `(b -> a -> b)^c`, `(a -> Bool)^f`) и
  значения с регионами (`a^v`, `c^d`) остаются привязанными к своему `scope`
  ([квалификаторы](../regions/#квалификаторы)).

## Обобщённые потребители и примеры

Модуль `Collections` предоставляет обобщённые функции, требующие только
минимального набора возможностей:

- `ToSeq`: `take`, `find`, `any`, `all`, `each_effect`, `fold_effect`
- `(Sized, ToSeq)`: `to_list`, `count`
- `Keyed`: `lookup_or`, `lookup_all` (с `ToSeq` по ключам)
- `(ToSeq, Member)`: `filter_members`, `all_in`
- `Indexed`: `endpoints`, `nth_from_end`

```flick
import Collections exposing (Keyed(..), Member(..), Sized(..), ToSeq(..))

// Works on any Keyed collection (HashMap with any witness, Trie, OrderedMap).
present_values : Keyed c String Int => List String -> c -> List (String, Int)
let present_values keys m = Seq.to_list (Collections.lookup_all keys m)

// Works on any ToSeq source (including infinite Seq) and any Member filter.
first_three_allowed : (ToSeq c a, Member s a) => s -> c -> List a
let first_three_allowed allowed source =
  Seq.to_list (Seq.take 3 (Collections.filter_members allowed source))

// Works on any ToSeq via `for` because `to_seq` yields a `Seq`.
sum_prefix : ToSeq c Int => Int -> c -> Int
let sum_prefix limit xs =
  let mut total = 0
  let mut seen = 0
  for n <- to_seq xs do {
    if seen >= limit then break
    total = total + n
    seen = seen + 1
  }
  total
```

Пользовательская конечная коллекция наследует `first`, `is_empty`, `last` и
`member` с коротким замыканием из реализаций по умолчанию:

```flick
import Collections exposing (Indexed(..), Member(..), Sized(..), ToSeq(..))

type Bag a = Bag (List a)

instance ToSeq (Bag a) a =
  let to_seq (Bag xs) = to_seq xs

instance Sized (Bag a) =
  let size (Bag xs) = size xs

instance Indexed (Bag a) a =
  let at index (Bag xs) = at index xs

instance Eq a => Member (Bag a) a

sample : (Bool, Option Int, Option Int, Bool)
let sample =
  let b = Bag [4, 5, 6]
  (is_empty b, first b, last b, member 5 b)
```

Потенциально бесконечный `Seq` не допускает `Sized`, а `HashSet` не допускает
`Functor`:

```flick fail C001
let n = Collections.size (Seq.from 0)
```

```flick fail C001
let bad = map (fun n -> n + 1) (HashSet.from_list EqHash.canonical [1, 2])
```
