---
title: "5. Типовое ядро HKT и классы типов"
---

## Вычисления и kinds

Представление вычисления — `Eff requirements errors result`. Результат стоит последним, чтобы `Eff r e` был конструктором kind `Type -> Type`.

```text
Unit, Never, Number, BigInt : Type
Option, List, Ref, Redacted, Schema : Type -> Type
Result, PureCodec : Type -> Type -> Type

Eff : ServiceRow -> ErrorRow -> Type -> Type
Layer : ServiceRow -> ErrorRow -> ServiceRow -> Type
Codec : ServiceRow -> ServiceRow -> Type -> Type -> Type
Errors : ErrorRow -> Type

FieldRow = Row Type
Region, CaptureRow, Symbol : Kind

Eff r e : Type -> Type
```

`FieldRow = Row Type` здесь обозначает равенство форм kinds, а не пользовательское объявление типа. `ServiceRow`, `ErrorRow` и `CaptureRow` — разные встроенные kinds. Labels полей имеют kind `Symbol`; service/error identities номинальны и не подменяются строковыми labels.

При генерации кода `Eff r e a` отображается в `Effect<A, ErrorUnion<E>, ServiceUnion<R>>`. Исходные rows имеют собственные номинальные идентификаторы, нормализацию и правила ограничений. TS unions служат выходным представлением; пустая строка в обоих случаях даёт `never`.

## Запись и объединение строк

В позиции отдельной строки используются `none`, переменная `r`, закрытая строка `{Users, Log}` либо открытая `{Users, Log | r}`. В стрелке внешние скобки строк опускаются: `a ->{Users, Log; NotFound} b` и `a ->{Users | r; NotFound | e} b`. Здесь `| r` и `| e` обозначают хвост строки. Без хвоста список закрыт. Порядок элементов несущественен, повторы одного номинального элемента нормализуются.

`r1 + r2` и `e1 + e2` обозначают объединение строк одного kind, например `Eff (r1 + r2) (e1 + e2) a`. Это встроенная операция row solver, не пользовательская type family. Сигнатура задаёт верхнюю границу: реализация может требовать меньше сервисов или допускать меньше ошибок. Расширение строки не превращает runtime-стрелку в чистую.

`Errors e` переводит строку ошибок в обычный тип значений: `Errors none = Never`, `Errors {NotFound} = NotFound`, а для нескольких элементов получается сумма номинальных ошибок. Это фиксированная проекция checker. Произвольный тип нельзя автоматически обратить в строку: для generic-моста нужна сохранённая строка `e` либо аннотация, для отдельной объявленной ошибки доступна singleton-строка.

## Пример HKT

```flick
class Functor (f : Type -> Type) =
  map :
    forall c d a b.
    (a -> b)^c -> ((f a)^d -> (f b)^(c + d))^c

instance Functor Option =
  map :
    forall c d a b.
    (a -> b)^c ->
    ((Option a)^d -> (Option b)^(c + d))^c
  let map f value =
    match value with
    | None -> None
    | Some x -> Some (f x)
```

Обычный `Functor.map` принимает pure callback. `c` ограничивает его captures, `d` — captures входного контейнера; результат консервативно сохраняет `c + d`, поскольку абстрактный f может отложить применение и удерживать input/callback. Partial application пока хранит только callback. Option вычисляет его сразу и удовлетворяет более широкому контракту через weakening верхней границы. `Eff r e` создаёт холодное описание и сохраняет captures. Eager List.map имеет более точный контракт раздела 6.

## Выбор instances и dictionaries

Frontend проверяет kinds, выводит типы и ограничения, разрешает instances и вставляет явные dictionary-аргументы. В runtime dictionary — значение с реализациями методов; HKT не требуют отдельного исполнителя типов.

```flick
class Show a =
  show : a -> String

render : Show a => a -> String
let render value =
  show value
```

```ts
interface Show<A> {
  readonly show: (value: A) => string
}

const render =
  <A>(dictionary: Show<A>) =>
  (value: A): string =>
    dictionary.show(value)
```

Похожая передача реализаций используется в PureScript. [Классы типов и сгенерированные dictionaries](https://book.purescript.org/chapter6.html).

## Требования к типовому ядру

- Higher-rank callbacks с вложенным `forall`, проверкой полных сигнатур и выводом типов внутри тела.

- Номинальные ADT, точные structural records с row polymorphism и исчерпывающий `match`.

- HKT с конструкторами типов и частичным применением.

- Multiparameter classes, superclasses и functional dependencies входят в первую версию; row solver имеет фиксированные детерминированные primitives.

- Уникальные canonical instances без overlap, orphans и локального implicit-поиска; владение определяется пакетом.

- Явные интерфейсы публичных модулей.

Associated types, пользовательские открытые и закрытые type families, instance chains, impredicative instantiation и общий higher-rank inference не входят в первую версию. Полиморфная рекурсия проверяется только по явно написанной полной `forall`-сигнатуре. SQL/UI используют встроенные row primitives, обычные HKT, ограниченные relations классов и типизированные AST.

Обобщение обычного чистого `let` и результата effectful binding нельзя механически считать одинаковым. Для состояния и FFI нужны правила generalization и value restriction, предотвращающие небезопасное разделение полиморфных изменяемых значений.

## Полиморфные callbacks

Higher-rank callbacks входят в язык сразу. Вложенный `forall` в полной сигнатуре означает, что вызываемая функция вправе применять один callback при разных типах. Обычные выражения по-прежнему получают локальный inference; на границе с полиморфным аргументом checker использует ожидаемую сигнатуру. HKT и ранг полиморфизма остаются разными свойствами.

| Правило | Следствие |
| - | - |
| Явный вложенный `forall` | Полиморфизм аргумента виден в полной сигнатуре; угадывать его по нескольким вызовам не нужно |
| Bidirectional checking | Checker выводит обычные типы и проверяет выражение против заданного полиморфного типа |
| Predicative instantiation | Обычная переменная типа инстанцируется мономорфным типом; произвольный `List (forall a. ...)` автоматически не допускается |
| Constraints внутри `forall` | При каждом применении callback передаются dictionaries для соответствующего типа |
| Rows во внешней области | Callback может быть полиморфным по результату, сохраняя один и тот же набор сервисов и ошибок |
| Проверка захватов и skolem escape | Полная сигнатура не разрешает скрыть mutable alias или выдать фиксированный тип за произвольный |
| Объём первой версии | Вложенные `forall` в аннотированных функциональных типах; общий higher-rank inference и impredicativity не обещаются |

Простой пример: один callback вызывается при `Number` и при `String` в одном теле.

```flick
apply_twice :
  (forall a. a -> a) -> (Number, String)
let apply_twice transform =
  (transform 42, transform "hello")

render_twice :
  (forall a. Show a => a -> String) -> (String, String)
let render_twice render =
  (render 42, render "hello")
```

Runtime-callback тоже может быть полиморфным:

```flick
decode_pair :
  (forall a. Schema a -> Unknown ->{r; SchemaError} a) ->
  Unknown -> Unknown ->{r; SchemaError} (User, Settings)
let decode_pair decode user_input settings_input =
  let user = decode (schema User) user_input
  let settings = decode (schema Settings) settings_input
  (user, settings)
```

Здесь `a` меняется при каждом вызове, а `r` принадлежит внешней сигнатуре. Для примера предполагаются объявленные `User`, `Settings` и их Schema.

Цена higher-rank — bidirectional checking, проверка жёстких типовых переменных, инстанцирование и диагностика escape. Полиморфная рекурсия и mutable generalization остаются отдельными проверками. Такой подход к аннотированным границам описан в [GHC](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/rank_polymorphism.html#type-inference); наличие общих сигнатур над функциями делает его естественным для Flick.

## Структурность records

Выбраны **точные закрытые structural records и полиморфизм строк полей**. `type alias User = { ... }` — прозрачный alias формы; номинальность вводится отдельным opaque/newtype объявлением. ADT номинальны. Неявного width subtyping нет: дополнительные поля принимаются только через явно открытый хвост строки либо удаляются явной проекцией.

| Модель | Что удобно | Цена |
| - | - | - |
| Точные structural records + открытые строки полей — выбрано | Литералы без конструкторов; функции принимают любую явно описанную дополнительную строку; точная форма пригодна для Schema | Нужны унификация строк полей, уникальность labels и правила constraints для открытого хвоста |
| Структурность с width subtyping | Большой объект проходит в API меньшей формы почти как в TS | Скрытые поля остаются в runtime; сложнее inference, higher-rank checking, equality и согласование с точными schemas |
| Номинальные records | Разные домены могут иметь одинаковые поля и свои instances | Дополнительные конструкторы/обёртки и преобразования на FFI; меньше удобства для повседневных данных |

Строка полей имеет kind `FieldRow`, эквивалентный `Row Type`; это не строка сервисов или ошибок. Labels уникальны; их порядок не входит в тип. В первой версии доступны встроенные `HasField`, `Lacks`, `Project`, `Remove`, `UnionDisjoint`, `Map` по конструктору kind `Type -> Type` и каноническое перечисление labels. Пользовательские instances для этих primitives запрещены. `UnionDisjoint` отвергает повтор label; объединение service/error rows, наоборот, нормализует повтор одного номинального элемента.

```flick
name :
  forall fields. { name : String | fields } -> String
let name value =
  value.name

project_name :
  forall fields. { name : String | fields } -> { name : String }
let project_name value =
  { name = value.name }
```

`name` допускает дополнительные поля через `fields`. `project_name` создаёт объект только с полем `name`; это не cast, скрывающий ключи. Обновление `{ record with field = value }` заменяет существующее поле значением того же типа. `Record.extend` требует `Lacks`; `Record.merge_disjoint` требует непересекающихся строк. Смена типа поля делается явными remove/extend либо новым record. Скрытого overwrite при spread нет. Вычисление значений полей следует порядку исходного кода.

### Records, instances и FFI

Два прозрачных aliases одной формы — один тип и не получают разные глобальные `Eq` или `Hash`. Для закрытого record действует canonical structural instance при наличии instances каждого поля. Открытая строка требует constraints на весь хвост. Compiler-owned structural instances нельзя переопределить для отдельного alias.

Для другой доменной семантики вводится nominal wrapper. На границе native Effect Equal/Hash такой wrapper имеет реальную runtime-оболочку с собственным протоколом. Её стирание допустимо только внутри участка, где compiler доказал отсутствие наблюдения identity/protocol через FFI или коллекции. Альтернативное сравнение без нового типа передаётся явным согласованным `EqHash` witness в dictionary-aware collection.

На входе из JS точный record по умолчанию отвергает лишние ключи; явно выбранная projection policy создаёт новый объект нужной формы. Обычный cast этого не делает. Представление record — plain JS object; недоверенная граница по умолчанию копирует поддерживаемые immutable данные и не отдаёт mutable alias исходного объекта. Прямой readonly façade без копирования допустим только по доверенному контракту ownership. `readonly` из `.d.ts` сам такого контракта не доказывает.

[Row polymorphism PureScript](https://github.com/purescript/documentation/blob/master/language/Types.md) показывает подход со строками полей. У [Effect Equal](https://effect.website/docs/v4/trait/equal) обычные объекты сравниваются структурно, поэтому незаметно сохраняемые дополнительные поля влияют на поведение коллекций.

## Выбранное ядро классов и цена альтернатив

Список решений состоит из независимых осей: число параметров класса; способ выражения связей между типами; выбор и владение instances; обобщение let-bindings. HKT и higher-rank callbacks уже выбраны и не требуют автоматически включать все остальные механизмы.

### Что даёт один параметр

```flick
class Show a =
  show : a -> String

unboxed type PrintableUser =
  | PrintableUser User

instance Show PrintableUser =
  show : PrintableUser -> String
  let show value =
    match value with
    | PrintableUser user -> user.name

render : Show a => a -> String
let render value =
  show value
```

PrintableUser — nominal wrapper, которым владеет пакет instance. Сам прозрачный record-alias User такого права для чужого класса не даёт.

`Show a` выбирает реализацию для одного типа. Параметр также может иметь kind `Type -> Type`: у `Functor f` он один — `f`. Поэтому однопараметрическое ядро поддерживает Functor, Foldable, Traversable, Monad и `Functor (Eff r e)`. Higher-rank callback с `forall a. Show a => ...` остаётся доступным.

Цена ограничения видна у отношения между двумя независимыми типами:

```flick
class Convert input output =
  convert : input -> Result ConversionError output
```

Здесь полезны и `Convert String Email`, и `Convert String UserId`. В однопараметрическом ядре это выражается отдельными функциями либо явными converter/codec-значениями. Наш `Codec rd re i a` уже позволяет передать нужное преобразование напрямую; ему не требуется единственный глобальный instance.

### Начальный набор

В первую версию входят **multiparameter classes, superclasses и FD**, unique canonical instances без overlap, package ownership и generalization доказанно чистых построений. Runtime-результаты мономорфны. Такой набор нужен для связей между параметрами SQL/UI eDSL; встроенные операции над rows снимают часть задач, которые PureScript-библиотеки решают сложным поиском instances.

Associated types, общие type families и relaxed value restriction по variance отложены за первую версию. Сравнения ниже объясняют цену этих механизмов; они не являются оставленными открытыми решениями.

## Параметры классов и зависимости между типами

| Механизм | Пример применения | Цена |
| - | - | - |
| Один параметр и superclasses | `Eq a`, `Hash a`, `Functor f`, `Traversable f` | Простой поиск instances; сложные отношения между независимыми типами выражаются явными значениями |
| Несколько параметров | `Convert source target` | Иногда оба типа приходится указывать явно из-за неоднозначности |
| Functional dependencies | Тип коллекции однозначно определяет тип элемента | Нужны улучшение constraints, проверка совместимости зависимостей и завершения поиска |
| Associated types | `Element collection`, тип результата операции | Solver должен нормализовать функции на типах и решать равенства |
| Общие type families | Вычисления над типами и более сложные DSL | Наибольшая цена в checker, интерфейсах модулей и диагностике |

### Multiparameter classes и functional dependencies

```flick
class Collection c a | c -> a =
  empty : c
  insert : forall captures. a^captures -> (c -> c)^captures
```

Qualifier у insert сохраняет значение a в частично применённой функции; конкретный тип коллекции также сохраняет captures своих элементов.

Зависимость `c -> a` читается: «конкретный тип коллекции определяет единственный тип элемента». Например, возможны heads `Collection UserSet User` и `Collection NameSet String`.

```flick
empty_users : UserSet
let empty_users =
  empty
```

Ожидаемый `UserSet` определяет и `User`. Без зависимости у метода был бы тип `Collection c a => c`, где `a` не указан результатом; требуется отдельное основание для однозначного выбора dictionary.

Пара `Collection UserSet User` и `Collection UserSet String` нарушает FD. Обратное направление не обещается: один `User` может храниться в разных коллекциях. После раскрытия aliases и нормализации встроенных row expressions переменные определяемой части instance должны входить в определяющую часть — strict coverage. Если определяющие позиции двух heads унифицируются, их определяемые позиции должны согласовываться. Context constraints не служат обходом coverage или способом выбрать между конфликтующими heads.

Multiparameter class не обязан иметь functional dependency. Для `Convert String Email` и `Convert String UserId` зависимость `input -> output` как раз неверна: String допускает оба результата.

Цена FD — уточнение переменных по constraints, проверка согласованности instances, coverage и завершения поиска. [GHC functional dependencies](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/functional_dependencies.html) показывает этот механизм на связи коллекции с элементом. Для Schema/Codec нельзя автоматически назначать единственный wire-format одному доменному типу.

### Associated types

Associated types исключены из первой версии. Для сравнения с выбранным FD тот же сценарий выглядел бы так:

```flick
class Collection c =
  type Item c

  insert : Item c -> c -> c
```

Instance для `UserSet` объявляет `Item UserSet = User`. Полная сигнатура общего метода имеет вид `Collection c => Item c -> c -> c`.

Это удобно, когда сущность определяет свои связанные типы: Repository — Entity и Key, Parser — Output, коллекция — Item. При неизвестном `c` выражение `Item c` может оставаться невычисленным. Из `Item c = User` нельзя восстановить единственный `c`.

Цена associated types — отдельный solver равенств и нормализации type expressions. Даже определения только внутри canonical instances добавляют новый механизм checker. Первая версия выражает такую связь через MPTC/FD или явный witness.

### Type families вне первой версии

```text
type family Element container =
  | List a -> a
  | Option a -> a
```

Здесь вычисление типов существует независимо от class dictionary и runtime-методов. Неизвестный аргумент может оставить `Element container` невычисленным. Потребуются правила применимости уравнений, их совместимости, нормализации и завершения.

[Type families GHC](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/type_families.html) включают как связанные определения внутри классов, так и самостоятельные семейства. FD и associated types можно сочетать, но это отдельные источники сложности.

Полноценный deriving Schema не зависит от принятия общих type families: compiler генерирует witnesses по определению типа и получает схемы generic/HKT-параметров явно.

## Coherence и альтернативные instances

Coherence означает, что одно ограничение получает согласованную реализацию. Для коллекций смена `Eq`, `Hash` или `Ord` между вставкой и поиском может нарушить их поведение.

| Политика | Выигрыш | Цена |
| - | - | - |
| Один instance без overlap и orphans | Предсказуемая семантика и совместимость библиотек | Для чужого класса и чужого типа нужна nominal-обёртка |
| Разрешённые orphan instances | Удобные пакеты интеграции | Две библиотеки могут предложить несовместимые instances |
| Overlap с приоритетами | Общая реализация и специализированные случаи | Усложняется поиск; добавление библиотеки способно изменить выбранную реализацию |
| Локальные неявные instances | Короткая запись локальной настройки | Нужны правила захвата dictionary и передачи значений между областями |
| Явные dictionaries | Альтернативное поведение видно в API | Больше аргументов; коллекция должна хранить выбранные Eq/Hash |

Принято: heads instances после раскрытия aliases не должны унифицироваться; их context не используется для устранения overlap. Все переменные context должны присутствовать в head. Рекурсивные class-constraints должны структурно уменьшаться без роста числа вхождений переменной; superclass graph ацикличен. Это консервативное условие завершения, без аналога `UndecidableInstances`. Превышение бюджета solver даёт диагностику и никогда не означает успешное разрешение. `Eq a => Eq (List a)` допустим. [Правила завершения instances GHC](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/instances.html#instance-termination-rules) служат ориентиром для такого консервативного ограничения, а не обещанием поддержать все расширения GHC.

### Overlap на примере

Предположим, объявлены два правила:

```text
Show a => Show (List a)
Show (List Number)
```

Для `List Number` подходят оба. Приоритет более специального instance выглядит удобным, но compiler должен решить, какое поведение получает уже скомпилированная generic-функция и может ли специализация выбрать другую реализацию.

[GHC описывает связь overlap со specialization](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/instances.html#overlapping-instances). В optimizer Flick несколько dictionaries одного типа требуют сохранять конкретно выбранный dictionary. Без overlap проще получить прямой method call и затем применить inlining.

### Orphan instance и внешняя библиотека

Пусть `Hash` определён стандартной библиотекой, а `ExternalId` — SDK. Пакет адаптера, не владеющий ни классом, ни исходным типом, не может объявить глобальный `Hash ExternalId` при no-orphan политике.

```flick
unboxed type LocalExternalId =
  | LocalExternalId ExternalId
```

Теперь адаптер владеет номинальным типом и может дать ему canonical instances. `unboxed` оставляет во время выполнения сам `ExternalId`: обёртка ничего не аллоцирует и уходит в SDK как есть, а идентичность и instances остаются у номинального типа. Прозрачный alias `type alias LocalExternalId = ExternalId` новой идентичности не создаёт. Другие пути — instance у владельца класса/типа или переданный явно dictionary.

Владение определяется **пакетом**: объявить instance может пакет класса либо пакет внешнего номинального конструктора **первого параметра head**. Прозрачный alias и structural record владельца не создают. Для MPTC владение только более поздним параметром недостаточно: пакет `LocalEmail` не получает право объявить чужой `Convert String LocalEmail`. Для этого нужен explicit codec/dictionary, wrapper первого параметра либо instance у владельца класса. Такой ограничительный anchor исключает конкурирующих владельцев разных параметров; цена — дополнительные wrappers в интеграциях. Binding не меняет идентичность foreign-типа.

### Локальные implicit instances

Можно предложить область, внутри которой `Eq String` означает сравнение без учёта регистра. Тогда требуется определить, какой dictionary захватывает closure и что получает коллекция после выхода из области. Лексический выбор можно сделать детерминированным, но type name уже не задаёт единственное поведение.

Для альтернативной сортировки достаточно явного comparator. Hash-коллекция должна получать и хранить согласованную пару Eq/Hash; смена окружающего instance не меняет семантику уже заполненной коллекции.

Выбраны canonical implicit instances, nominal wrappers и явные alternative dictionaries. Orphans, overlap с приоритетами и локальные implicit instances в первой версии запрещены. Набор instances определяется графом зависимостей пакета, а не порядком или формой `import`; конфликты диагностируются при проверке всего доступного графа.

## Generalization при наличии Ref

Обобщение означает, что тип binding можно заново инстанцировать при каждом использовании. У `identity : forall a. a -> a` два вызова могут независимо выбрать `Number` и `String`. У одной изменяемой ячейки должен оставаться один тип содержимого. Явная сигнатура `forall` не может сделать небезопасное разделение состояния допустимым.

```flick
invalid : Unit ->{} String
let invalid () =
  let cell = Ref.make None
  Ref.set cell (Some 42)
  match Ref.get cell with
  | Some value -> String.trim value
  | None -> ""
```

Программа должна быть отвергнута: запись `Some 42` фиксирует содержимое ячейки как `Option Number`, а `String.trim` требует `String`. Если бы `cell` получила `forall a. Ref (Option a)`, запись и чтение могли бы выбрать разные типы для одного объекта. Проблема полиморфных изменяемых ячеек и relaxed value restriction разобрана в [руководстве OCaml](https://ocaml.org/manual/5.4/polymorphism.html).

| Правило обобщения | Цена реализации | Цена для автора программы |
| - | - | - |
| Только синтаксические значения | Самое простое консервативное правило | Иногда приходится добавлять функцию-обёртку для безопасного выражения |
| Доказанно чистое построение значения | Использует анализ pure/runtime и свободных переменных окружения | Меньше вынужденных функций-обёрток; требует правдивых контрактов FFI |
| Дополнительное обобщение по variance | Нужен анализ variance, включая абстрактные типы | Допускает больше безопасных программ; `Ref a` остаётся инвариантным |
| State effects и regions | Существенно более сложная система состояния | Позволяет скрывать локальную мутацию за чистым API |

Обобщаются **доказанно чистые построения значений**, только по переменным, не свободным в окружении и не связанным с локальными skolems, region/capture obligations или неоднозначными constraints. Результат runtime-binding не обобщается даже при `->{}`. Полная сигнатура не отменяет эти проверки. Relaxed value restriction по variance в первой версии не используется; mutable containers инвариантны. Доверенный pure FFI обязан соблюдать тот же контракт чистоты.

```flick
make_empty : forall a. Unit ->{} Ref (Option a)
let make_empty () =
  Ref.make None

use_separate_cells : Unit ->{} Unit
let use_separate_cells () =
  let numbers = make_empty ()
  let names = make_empty ()
  Ref.set numbers (Some 42)
  Ref.set names (Some "Ada")
```

Значение функции `make_empty` полиморфно: создание замыкания чистое. Каждый runtime-вызов выделяет новую ячейку; сама `numbers` мономорфна и хранит только `Option Number`, а `names` — только `Option String`. Это различие сохраняет полезный полиморфизм фабрик, не разрешая менять тип уже созданного состояния.

### Generalization на трёх небольших примерах

**Синтаксические значения и доказанная чистота.**

```flick
make_identity : Unit -> (a -> a)
let make_identity () =
  fun value -> value

identity_again : forall a. a -> a
let identity_again =
  make_identity ()
```

Правая часть `make_identity ()` — вызов, а не синтаксическое значение. Строгое правило «обобщать только значения» отвергнет требуемую полиморфную сигнатуру `identity_again`; добавление явного `forall` не отменяет правило. Анализ доказанной чистоты разрешит её: вызов чистый, состояние не выделяется, переменная `a` не захвачена из окружения. Higher-rank inference здесь не требуется.

**Чистое описание вычисления и его runtime-результат.**

```flick
fresh_cell : forall a. Eff none none (Ref (Option a))
let fresh_cell =
  task { Ref.make None }

use_tasks : Unit ->{} Unit
let use_tasks () =
  let numbers = await fresh_cell
  let names = await fresh_cell
  Ref.set numbers (Some 42)
  Ref.set names (Some "Ada")
```

`task` создаёт холодное описание. Поэтому `fresh_cell` можно обобщить: каждый `await` заново выполняет `Ref.make`. Полученные ячейки снова мономорфны. `await` не означает кеширование результата и не запускает отдельный fiber.

```flick
capture_cell : Ref (Option a) -> Eff none none (Ref (Option a))
let capture_cell cell =
  task { cell }
```

Здесь повторные запуски возвращают одну захваченную ячейку. Тип `a` связан с аргументом `cell`: локальное описание не может обобщить переменную, свободную в окружении. Аналогично memoization не должна сохранять полиморфизм свежего результата, если превращает его в разделяемое состояние. Холодность `Eff` сама по себе не доказывает отсутствие захватов.

**Что дополнительно разрешает variance.**

```flick
logged_none : Unit ->{Log; none} Option a
let logged_none () =
  Log.info "created an empty result"
  None

use_logged_none : Unit ->{Log; none} (Option Number, Option String)
let use_logged_none () =
  let empty = logged_none ()
  (empty, empty)
```

При консервативном запрете обобщать runtime-результаты `empty` получает один неизвестный тип `Option a`; использовать его в паре сразу как `Option Number` и `Option String` нельзя. Relaxed value restriction может обобщить `a` в ковариантном неизменяемом `Option a`, сохранив один вызов `Log.info`. Это дополнительная выразительность, а не способ выполнить логирование чисто.

| Положение переменной | Пример | Следствие для relaxed value restriction |
| - | - | - |
| Ковариантное | Неизменяемый `Option a` | Кандидат на дополнительное обобщение |
| Инвариантное | `Ref a` | Разделяемое изменяемое содержимое не обобщается |
| Одновременно вход и выход | `a -> a` | Одной проверки ковариантности недостаточно |
| Абстрактный тип | `Container a` из другого модуля | Variance должна быть проверена и сохранена в интерфейсе |

Правило первой версии — **доказанная чистота + свободные переменные окружения + проверка lifetime и однозначности constraints**. Пример `logged_none` выше намеренно отвергается; он показывает цену отказа от variance-generalization. Дополнительное обобщение по variance вынесено за первую версию.

## Deriving и законы классов

`deriving Eq, Ord, Hash, Show` создаёт class instances; полноценный `deriving Schema` создаёт runtime-свидетельство данных. Deriving Schema охватывает records, ADT, tuples, generics, рекурсивные типы, refinements и расширение через явные witnesses. Он не объявляет одноимённый type class и не требует общего языка type families. Подробный контракт — в разделе 9.

Проверка instance подтверждает его тип, но не доказывает законы класса. Генерация `Eq` и `Hash` должна использовать согласованную семантику полей. Пользовательские реализации требуют отдельных проверок законов; сериализация и формат хранения задаются явным codec, а не единственным глобальным instance.

## Monad и объединение строк

```text
bind :
  forall c d m a b.
  Monad m => (m a)^c ->
  ((a -> (m b)^d)^d -> (m b)^(c + d))^c

bind_eff :
  forall c d r1 r2 e1 e2 a b.
  (Eff r1 e1 a)^c ->
  ((a -> (Eff r2 e2 b)^d)^d ->
    (Eff (r1 + r2) (e1 + e2) b)^(c + d))^c
```

Capture rows задают верхние границы; результат сохраняет captures исходного описания и продолжения. Partial application удерживает пока только первый аргумент.

Обычный `Monad (Eff r e)` фиксирует `r` и `e`; допускается их явное или выведенное расширение до общей строки. Прямые runtime-вызовы используют отдельное встроенное sequencing с объединением строк, без обращения к instance `Monad`.

Frontend вычисляет объединения requirements/errors для прямых вызовов и сохраняет captures описаний и callbacks. Indexed/graded abstractions и generic `do` не входят в первую версию; HKT, `bind` и обычные combinators доступны.

**Решение типового ядра:** MPTC/superclasses/FD с strict coverage и структурным завершением; package ownership по классу или первому номинальному параметру; no overlap/orphans/local implicits; фиксированные row primitives; обобщение только доказанно чистых построений. Higher-rank callbacks проверяются по явным nested `forall`; эта возможность не ослабляет ни coherence, ни ограничения состояния.
