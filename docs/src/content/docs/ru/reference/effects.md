---
title: "Эффекты, ошибки и сервисы"
source_hash: cbb4ecfb6a01d4c4
---

Компилятор проверяет, где допустимы эффектные вызовы, какие сервисы им нужны
и какие ошибки они могут вернуть (см. [Прямые эффектные вызовы](../../spec/01-direct-calls/),
[Значения вычислений и их выполнение](../../spec/02-computations/),
[Ошибки, сервисы, ресурсы и конкурентность](../../spec/05-errors-and-concurrency/)).
Типы и кинды описаны в разделе [о типах](../types/).

## Стадии

Каждая стрелка каррированной функции — отдельная *стадия*. Эффектный вызов
происходит при передаче аргумента эффектной стрелке:
в `A -> B ->{R; E} C` первое применение чистое, а второе — эффектное;
в `A ->{R; E} (B -> C)` эффектный вызов происходит уже после первого аргумента.
Тело функции выполняется на последней стадии её определения, поэтому
вызовы из тела учитываются в последней стрелке сигнатуры.

```flick
# service Log =
#   info : String ->{} Unit
#
greet : String -> String ->{Log; none} String
let greet prefix name =
  Log.info name
  "\{prefix}\{name}"

hello : String ->{Log; none} String
let hello =
  greet "Hello"
```

Эффектный вызов в чистом коде вызывает `FLICK-E001`. Это относится к функции
с последней стрелкой `->`, к переданной функции, от которой ожидается
чистота, и к инициализации модуля. Пустые строки не делают вызов чистым:
вызов функции `a ->{} b` из чистой функции по-прежнему вызывает `FLICK-E001`,
а передача её туда, где ожидается `a -> b`, — `FLICK-T003`.
Значения верхнего уровня вычисляются при загрузке модуля, поэтому их
определения должны быть чистыми. `task` на верхнем уровне допустим:
его тело выполняется позже.

В `let user = Users.get id` переменная связывается с результатом вызова —
значением `User`. Аргументы вычисляются, а стадии выполняются строго
слева направо.

## Требования вызовов и границы сигнатуры

Строки всех вызовов на одной стадии объединяются, а сигнатура задаёт
верхнюю границу требований (см. [Запись и объединение строк](../../spec/03-types-and-classes/#запись-и-объединение-строк)).
Реализация функции может требовать меньше сервисов и допускать меньше видов
ошибок, чем указано в сигнатуре, но не больше. При превышении границы возникает
`FLICK-E002`; сообщение перечисляет все лишние сервисы и ошибки:

```flick
# type UserId = UserId String
# type alias User = { name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# service Log =
#   info : String ->{} Unit
#
load_name : UserId ->{Users, Log; NotFound} String
let load_name id =
  let user = Users.get id
  Log.info user.name
  user.name
```

Порядок элементов строки не имеет значения, повторяющиеся элементы
учитываются один раз. `r1 + r2` объединяет две строки одного кинда:

```flick
both : Eff r1 e1 a -> Eff r2 e2 b ->{r1 + r2; e1 + e2} (a, b)
let both left right =
  (await left, await right)
```

Для переданной функции используются строки из ожидаемого типа.
Если это переменные строк вызываемой функции, их значения определяются
по вызовам в теле переданной функции:

```flick
# service Log =
#   info : String ->{} Unit
# each : (a ->{r; e} Unit) -> List a ->{r; e} Unit
# let each f xs = List.each_effect f xs
#
log_all : List Float ->{Log; none} Unit
let log_all numbers =
  each (fun n -> Log.info "number") numbers
```

Если ожидаемый тип не задан, последняя стрелка лямбды становится эффектной
тогда и только тогда, когда в её теле есть эффектный вызов.
Поэтому `fun x -> normalize_name x` имеет тип `String -> String`,
`fun x -> task { Users.get x }` — чистая функция, создающая `Eff`, а
`fun x -> Users.get x` имеет тип `UserId ->{Users; NotFound} User`.

Там, где ожидаются эффектная функция или `Eff`, можно передать значение,
которому нужно меньше сервисов и которое возвращает меньше ошибок.
Чистую функцию тоже можно передать вместо эффектной: компилятор
преобразует её в эффектную. Обратного преобразования нет:
эффектная функция никогда не считается чистой.

## Пустые строки: `none`

`none` — литерал пустой строки: он обозначает отсутствие сервисов или
ожидаемых ошибок (см. [Unit, Never и пустые строки](../../spec/02-computations/#unit-never-и-пустые-строки)).
Он подходит любому кинду строк. В зависимости от позиции это может быть
строка сервисов, ошибок или захватов. Все записи ниже обозначают
закрытую строку без элементов и хвоста:

| Запись | Где используется |
| - | - |
| `none` | любая позиция строки: `->{none; E}`, `Eff none none a`, `Layer none none {Clock}` |
| ничего | одна сторона стрелки: в `->{; E}` нет сервисов, в `->{R}` нет ошибок |
| `->{}` | стрелка с двумя пустыми строками |
| `{}` | строка в фигурных скобках: `Eff {} {} a` |

В стрелке перед `;` всегда записывается строка сервисов, а после него —
строка ошибок. Поэтому `->{; AppError}` и `->{none; AppError}` обозначают
один тип. Без `;` строка всегда содержит сервисы:
`->{Log}` означает `->{Log; none}`. Для ошибок нужно оставить `;`,
как в `->{; AppError}`.
`flick fmt` опускает пустые строки в стрелках: `->{none; E}` становится
`->{; E}`, `->{Log; none}` — `->{Log}`, а `->{none; none}` — `->{}`.
Завершающий `;` перед пропущенной строкой ошибок тоже удаляется.
Автоматически добавляемые сигнатуры используют те же сокращения.
В остальных позициях `none` сохраняется, например в `Eff none none a`.

```flick
# service Log =
#   info : String ->{} Unit
#
fail : String ->{; AppError} a
# let fail message = App.fail message

log_line : String ->{Log} Unit
# let log_line line = Log.info line
```

Пустые строки не делают вызов чистым. `a ->{} b` остаётся эффектной
стрелкой: такая функция может выполнять ввод-вывод, хранить состояние,
завершаться из-за дефекта или прерываться. Она лишь не запрашивает сервисов
и не объявляет ожидаемых ошибок. Это не `a -> b`: её вызов в чистом коде
вызывает `FLICK-E001`, а передача туда, где ожидается `a -> b`, —
`FLICK-T003`. У чистой стрелки строк вообще нет; это не то же самое,
что две пустые строки.

`none` — не тип. Это не `Unit` (одно значение), не `Never` (нет значений)
и не `None` из `Option`. Записи `x : none` и `none -> Float` вызывают
`FLICK-T002`, как и `Eff Unit Unit a`: у `Unit` кинд `Type`, а не кинд
строки. Внутри строки `none`, как и переменная строки, обозначает строку
целиком. Поэтому `{Log, none}` вызывает `FLICK-T002`; нужно писать `{Log}`.

Правила проверки `none`:

- `Errors none` — это `Never`. Если `Errors e` с неизвестным `e`
  сопоставляется с `Never`, `e` становится `none`.
- Сигнатура задаёт верхнюю границу требований. Поэтому тело без требований
  подходит для любой строки, а функцию с `none` можно использовать там,
  где разрешено больше сервисов или ошибок.
- Строками связаны только `->` и `Eff`. Под любым другим конструктором
  строки должны совпадать: `List (Eff {Log} none Unit)` не подходит как
  `List (Eff {Log, Clock} none Unit)` (`FLICK-T003`).
- Параметр неизвестного типа, применённый к аргументу, считается чистой
  функцией, поэтому `(fun h -> h ()) f` с эффектной `f` — это `FLICK-T003`.
  Эффектный колбэк получает тип из ожидаемого типа или из сигнатуры.
  Ошибка указывает на `h` и предлагает аннотацию,
  `fun (h : Unit ->{Log} Unit) -> h ()`.
- Если к концу объявления ограничения не определили строку, она становится
  `none`. Например, выведенная функция, которая не вызывает сервисы,
  получает пустую строку сервисов, а не свободную переменную строки.
- В сгенерированном коде пустая строка представлена как `never`
  в `Effect<A, E, R>`: так [Effect](https://effect.website/docs/v4/getting-started/the-effect-type)
  обозначает отсутствие требований или ошибок.

## `task` и `await`

`task { body }` создаёт `Eff r e a`, но не запускает вычисление.
Вызовы из тела определяют строки `r` и `e`. `await t` выполняет `t`
один раз в текущем файбере и снимает ровно один слой: ожидание
`Eff r e (Eff r2 e2 a)` даёт `Eff r2 e2 a`. Применение `await`
к значению, которое не является `Eff`, вызывает `FLICK-T003`.

`Effect.void : Eff none none Unit` — холодное вычисление, которое возвращает
`()`, не требует сервисов и не имеет ожидаемых ошибок. Передавайте его туда,
где аргумент `Eff` не должен ничего делать: само `()` — значение, а не
вычисление. `await Effect.void` запускает его; то же значение можно ожидать
повторно или передать в `Effect.all Effect.Sequential [Effect.void, Effect.void]`.

Функция, принимающая `Eff`, сама решает, когда его выполнять.
Но если в качестве аргумента записан эффектный вызов, он выполнится
ещё до начала работы функции. В этом случае предупреждение `FLICK-E003`
предлагает передать `task { … }`.

Функция может сама принимать аргумент незапущенным: параметр с пометкой
`~`, `~(Eff r e a)`, получает записанный там runtime-вызов как вычисление,
которое создал бы `task { … }`. Комбинаторы стандартной библиотеки,
выполняющие вычисление, помечают его, поэтому `Effect.or_die (Database.query q)`,
`timeout 500 (Http.get url)` и `Effect.retry policy (fetch id)` обходятся
без `task`; значение `Eff`, переданное туда, передаётся как есть.

```flick
twice : ~(Eff r e Int)^c ->{r; e} Int
let twice job = await job + await job
```

- `twice (count ())` выполняет `count ()` дважды, по разу на каждый `await`.
- Пометка не входит в тип: она относится к сигнатуре, хранится в интерфейсе
  модуля и действует там, где функция вызвана по имени. Конвейер `x |> f`
  по-прежнему сначала вычисляет `x`.
- `~` ставится только на параметр типа `Eff` (`FLICK-T076`).

## `lazy`

`lazy { body }` создаёт `Lazy r e a`. Как и `task`, он пока ничего не
выполняет, но первый `await` выполняет тело — в том файбере, который ждёт, —
а каждый следующий `await` получает тот же исход, не выполняя тело заново:
значение, типизированную ошибку или дефект. `lazy e` — это `lazy { e }`.

```flick
# type error Missing = Missing
# service Users =
#   find : Int ->{; Missing} String
names : Int ->{Users, Async; Missing} (String, String)
let names id =
  let name = lazy { Users.find id }
  (await name, await name)
```

- Создание выделяет ячейку, как `Ref.make`, поэтому это runtime-вызов
  (`FLICK-E001` в чистой функции).
- `await` значения `Lazy r e a` требует `{Async | r}` и может завершиться
  ошибками `e`: тело может выполнять другой файбер, и тогда `await` ждёт его.
  Файберы, одновременно ожидающие одно значение, разделяют одно выполнение.
- Прерывание не запоминается. Если файбер, выполняющий тело, прерван,
  следующий `await` выполнит тело снова; ожидавшие его файберы прерывания
  не видят.
- Тело может выполниться не в том файбере, где создано значение, поэтому
  нужен `Detached r`: без операций `control` (`FLICK-E017`).
- Значение, которое никто не ждёт, тело не выполняет.
- `Lazy` хранит захваченное телом и ошибку, с которой оно завершилось,
  поэтому не может пережить `scope` удерживаемого ресурса.

`let lazy x = e` связывает ленивое значение, принадлежащее только своей
функции, как `let mut` связывает переменную: оно читается только как
`await x`, который в первый раз выполняет `e`, а потом возвращает его исход,
и не требует `Async` — другой файбер до него не дотянется. Замыкание или
`task` не могут его использовать (`FLICK-G005`); общее значение создаётся
через `lazy { … }`. `let lazy mut` можно присвоить, и значение, присвоенное
до первого `await`, означает, что `e` не выполнится.

```flick
# service Config =
#   load : Unit ->{} String
banner : Bool ->{Config} String
let banner verbose =
  let lazy config = Config.load ()
  if verbose then await config else "quiet"
```

Формальные правила — в `docs/formal/lazy.md`.

## Ожидаемые ошибки

`type error E = E T …` объявляет ошибку с единственным конструктором `E`.
Модуль и имя определяют номинальную идентичность ошибки, а `T …` — поля
конструктора. У `type error E` полей нет. Данные ошибки могут жить дольше
кода, в котором она возникла. Поэтому ячейка (`Ref`), вычисление (`Eff`),
слой, эффектная функция или `Unknown` в полях ошибки вызывают `FLICK-E005`,
в том числе если они входят в состав других типов.

У ошибки могут быть параметры типа. Например, `type error RetryExhausted e =
RetryExhausted e Float` сохраняет последнюю причину, а `type error SendFailed a =
SendFailed a` возвращает значение, которое не удалось отправить.
Строка различает ошибки по имени, а не по аргументам, поэтому содержит
не более одной `RetryExhausted`. Если в одной строке встречаются две такие
ошибки, их аргументы должны совпадать (`FLICK-T003`). Представление
в TypeScript устроено так же: `_tag` ошибки не зависит от её аргументов,
объединение ошибок содержит каждый тег один раз, а `catchTag` сужает его
до одного типа.

`try … with` обрабатывает ошибки своего тела. Ошибка удаляется из строки
только тогда, когда ветвь без условия `when` охватывает все её значения.
Ветвь с условием или частичный образец оставляет ошибку в строке.
Переменная или `_` соответствует любой ошибке как значению `Errors e`, где
`e` — все ошибки тела: как одну ошибку её можно использовать, только если тело
бросает лишь её.
Требования обработчиков добавляются к требованиям всего выражения:

```flick
# type UserId = UserId String
# type alias User = { name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# type error DbUnavailable = DbUnavailable
# find_user : UserId ->{Users; NotFound, DbUnavailable} User
# let find_user id = Users.get id
#
find_optional : UserId ->{Users; DbUnavailable} Option User
let find_optional id =
  try
    Some (find_user id)
  with
  | NotFound _ -> None
```

Ветвь с образцом, который соответствует не ошибке, вызывает `FLICK-E007`.

`Errors e` преобразует строку ошибок в тип: `Errors none` — это `Never`,
`Errors {NotFound}` — это `NotFound`. `from_result` завершает вычисление
с ошибкой из `Err`. Для `Result NotFound a` строка ошибок — `{NotFound}`.
`Result String a` не подходит: вместо строки нужна объявленная ошибка:

```flick
# type alias User = { name : String }
# type error NotFound = NotFound String
#
checked : Result NotFound User ->{; NotFound} User
let checked result =
  from_result result
```

`match` разбирает значение `Errors e` по ошибкам его строки: ветвь
`NotFound id` сопоставляется с `NotFound` из строки. Ветви без условия
покрывают каждую ошибку закрытой строки, иначе `match` вызывает `FLICK-T009`,
а исправление добавляет недостающие ветви; строке с переменной нужна ещё ветвь
для остальных ошибок, а ветвь с ошибкой вне закрытой строки вызывает
`FLICK-T003`. `match` чист и не убирает ошибок из строки. Если ошибки значения
ещё не известны, как у переменной `try`, их задают ветви, если только ни одна
ветвь не сопоставляется с любой ошибкой (`FLICK-T057`):

```flick
# type alias UserId = String
# type error NotFound = NotFound UserId
# type error DbUnavailable = DbUnavailable
# service Users =
#   get : UserId ->{; NotFound, DbUnavailable} String
#
problem : Errors {NotFound, DbUnavailable} -> String
let problem e =
  match e with
  | NotFound id -> "no user " ++ id
  | DbUnavailable -> "try again later"

name_or_problem : UserId ->{Users} String
let name_or_problem id =
  try Users.get id with
  | e -> problem e
```

`Fail.fail` завершает вычисление с ошибкой напрямую. Вторая строка стрелки —
это эффект `Fail`: `->{Fail {NotFound}; }` то же, что `->{; NotFound}`, а ветка
`try` его ловит, в виде `| NotFound _ ->` или полностью
`| Fail.fail (NotFound _) ->`:

```flick
# type error NotFound = NotFound String
#
find : String ->{Fail {NotFound};} Int
let find name = Fail.fail (NotFound name)

found : String ->{} Int
let found name =
  try find name with
  | Fail.fail (NotFound _) -> 0
```

## Сервисы

В `service` перечисляются операции с полными сигнатурами. При выполнении
операции среда выполнения ищет сервис, поэтому у каждой операции должна
быть эффектная стрелка (`FLICK-E006`). У `S.op` сервис `S` добавляется
к требованиям каждой такой стрелки. Например, если в `Users` объявлено
`get : UserId ->{; NotFound} User`, то тип `Users.get` —
`UserId ->{Users; NotFound} User`. Операции сервиса — не методы класса:
методу класса, например `show`, сервис не нужен.

`S.Impl` — запись операций с сигнатурами из объявления сервиса.
`Layer.succeed S impl` создаёт из неё слой. Имя сервиса здесь
используется как значение:

```flick
# js_now : Unit ->{} Float
# let js_now () = 0.0
#
service Clock =
  now : Unit ->{} Float

clock_impl : Clock.Impl
let clock_impl =
  { now = js_now }

clock_layer : Layer none none {Clock}
let clock_layer =
  Layer.succeed Clock clock_impl
```

`type alias` сервиса или ошибки означает в строке эффектов сам сервис или
ошибку: после `type alias Time = Clock` строка `->{Time}` та же, что
`->{Clock}`. Псевдоним целой строки — это [набор](#наборы-сервисов-и-ошибок).
Псевдоним чего-то другого в строке — ошибка (`FLICK-T002`).
Псевдоним — имя типа, а не пространство имён: как псевдоним типа данных не
открывает его конструкторы, так псевдоним сервиса не открывает его операции
и `S.Impl`. Это делает [реэкспорт](../modules/): `Atom.Registry`, который
`Atom` реэкспортирует из `Reactivity.Atom`, — сам этот сервис, поэтому
работают `Atom.Registry.get`, `Atom.Registry.Impl` и
`Layer.succeed Atom.Registry impl`.

## Наборы сервисов и ошибок

`type alias` строки эффектов называет набор сервисов и ошибок. `...` добавляет
в строку элементы набора, простое имя добавляет один элемент, а `-` убирает его:

```flick
# type UserId = UserId String
# type alias User = { name : String }
# type error UserNotFound = UserNotFound
# type error InvalidEmail = InvalidEmail
# type error OrderNotFound = OrderNotFound
# type error Boom = Boom
# service Log =
#   info : String ->{} Unit
# service Users =
#   get : UserId ->{; UserNotFound} User
# service Billing =
#   charge : Float ->{} Unit
#
type alias UserErrors = {UserNotFound, InvalidEmail}

type alias Reads = {Log, Users; ...UserErrors}

type alias Orders = {...Reads, Billing; OrderNotFound}

type alias Quiet = {...Orders, -Log; -InvalidEmail}

load : UserId ->{...Orders} User
let load id = Users.get id

wider : UserId ->{...Orders, Async | r; Boom} User
let wider id = load id
```

Набор раскрывается там, где назван: `->{...Reads}` — та же строка, что
`->{Log, Users; UserNotFound, InvalidEmail}`. Поэтому композиции не нужно
своих правил:

- Наборы могут ссылаться друг на друга в любом порядке, а элемент из двух наборов
  считается один раз. Набор не может содержать сам себя (`FLICK-R007`).
- Что требует меньше, подходит туда, где разрешено больше, как и для
  [любой строки](#требования-вызовов-и-границы-сигнатуры): функция с `{...Reads}` проходит там,
  где ждут функцию с `{...Orders}`. Под другим конструктором строки должны совпадать,
  поэтому `List (Eff {...Reads} none Unit)` — не
  `List (Eff {...Orders} none Unit)` (`FLICK-T003`).
- Ветка `try` ловит одну ошибку набора, остальные остаются в строке.

В скобках набор пишется с `...`, а один элемент без них; в обоих случаях ошибка —
`FLICK-T002` с исправлением. Вне скобок псевдоним означает всю строку:
`Layer none none Services`, `Eff Services Errs a`, `Services + r`. Строка, которая
вливает или убирает набор, может обойтись без `;`: у набора могут быть свои ошибки.

Набор из одних ошибок стоит по любую сторону `;`, набор с сервисами только перед
ней (`FLICK-T002`). Стрелка и `Eff` переносят ошибки набора в свою строку ошибок.
У других типов со строкой, например `Layer {...Orders} none {Log}`, для них нет
места (`FLICK-T002`): назовите сервисы отдельным псевдонимом.

Оператор `-` удаляет элементы из всей строки в фигурных скобках независимо
от своего положения. При этом сервисы удаляются слева от `;`, а ошибки — справа. `-...Reads` убирает весь набор. Всё
это `FLICK-T002`, чтобы опечатка или переименование не прошли незаметно:

- убрать то, чего в строке нет;
- убрать из переменной строки элемент с аргументами, как в `{-Store Int | r}`:
  переменная может держать `Store` с другим типом, поэтому уберите все его
  экземпляры, `{-Store | r}`, или напишите `{Store Int | r}` и оставьте `r`;
- написать элемент и убрать его в одной строке, как в `{Log, ...Reads, -Log}`;
- `...r`, потому что переменная строки идёт после `|`; влить псевдоним с хвостом,
  потому что `| r` пишется там, где набор используется; `opaque type alias` строки.

Над переменной строки `-` говорит, что функция даёт сама. `{-Log | r}` — это
`r` без `Log`, держит его `r` или нет, поэтому вызывающий должен всё из своей
строки, кроме `Log`:

```flick
# service Log =
#   info : String ->{} Unit
# service Clock =
#   now : Unit ->{} Float
#
quiet : Layer none none {Log} -> Eff r none a ->{-Log | r} a
let quiet layer work = provide layer { await work }

timed : Layer none none {Log} -> Eff {Clock, Log} none a ->{Clock} a
let timed layer work = quiet layer work
```

Без `provide` телу `quiet` был бы нужен `Log`, когда `r` его держит, а сигнатура
этого не разрешает (`FLICK-E002`). С ошибками так же после `;`: `->{; -NotFound | e}`
над `try`, который обрабатывает `NotFound`.

Обработчик находит сервис по имени, а не по аргументам. `provide` слоя
`Store Int` отвечает на каждую операцию `Store` в своём теле, поэтому телу может
быть нужна переменная строки, только если она не держит другого экземпляра
`Store`: из неё он убран, `Eff {-Store | r} …`, или экземпляр назван рядом с ней,
`Eff {Store Int | r} …`, и тогда `Store` в ней только `Store Int`. Переменная,
которая может держать `Store String`, получила бы вместо него `Store Int` из слоя;
это `FLICK-E019`.

У набора могут быть параметры, как в `type alias Db s = {Scope s, Log}`.

Набор расширяет сигнатуру: вызывающий код должен предоставить его целиком, и тесты
тоже. Используйте наборы на границах модулей, где многим функциям нужно одно и то же.
Маленькой функции оставьте точную строку или дайте её вывести.

## Слои и `provide`

`Layer inputs errors outputs` создаёт сервисы `outputs`, используя сервисы
`inputs`, и может завершиться с ошибками `errors`. В `provide layer { body }`
слой строится до выполнения тела. Сервисы, созданные слоем, доступны телу;
самому блоку нужны входные сервисы слоя и остальные сервисы тела.
Строка ошибок блока объединяет ошибки слоя и тела.

```flick
# type UserId = UserId String
# type alias User = { name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# service Log =
#   info : String ->{} Unit
# service Config =
#   url : Unit ->{} String
# type error ConnectError = ConnectError
# users_live : Layer {Config} {ConnectError} {Users}
# let users_live = Layer.effect Users (task { Fail.fail ConnectError })
# load_name : UserId ->{Users, Log; NotFound} String
# let load_name id = (Users.get id).name
#
application : UserId ->{Config, Log; NotFound, ConnectError} String
let application id =
  provide users_live {
    load_name id
  }
```

Если `users_live` имеет тип `Layer {Config} {ConnectError} {Users}`,
а `load_name` требует `{Users, Log}` и допускает `{NotFound}`, то блоку
нужны сервисы `{Config, Log}`, а его строка ошибок —
`{NotFound, ConnectError}`.

`Layer.merge a b` создаёт сервисы обоих слоёв. Если оба слоя создают
один и тот же сервис, возникает `FLICK-E004`. `Layer.override a b`
тоже объединяет слои, но разрешает `b` заменить сервисы из `a`.

Время жизни сервисов, созданных `provide`, ограничено этим блоком. Работа,
зарегистрированная у владельца до закрытия его области (ребёнок `Fiber.spawn`,
`FiberSet.run`, финализатор `Scope.add_finalizer`, обработчик сервера),
исполняется с сервисами того места, где её запустили. Поэтому каждый из них
должен приходить от поставщика, который переживает этого владельца: от
`provide` вокруг области владельца или от вызывающего функции, в теле
которой открыта эта область. Иначе — `FLICK-G003`:

```flick fail G003
# service Greeter =
#   greet : String ->{} String
# report : Failure e ->{} Unit
# let report _ = ()
# greeter : Layer none none {Greeter}
# let greeter = Layer.succeed Greeter { greet = fun name -> name }
#
leaked : Unit ->{} Unit
let leaked () = scope outer {
  provide greeter {
    // FLICK-G003: ребёнок `outer` пользовался бы `Greeter` после того,
    // как `provide` его освободил
    ignore (Fiber.spawn outer report (task { Greeter.greet "child" }))
  }
}
```

Если функция получает владельца и сервисы от вызывающего кода, ограничение
`Fiber.Provided` требует, чтобы сервисы оставались доступны до закрытия области
владельца. Это условие проверяется при каждом вызове:

```flick
# service Greeter =
#   greet : String ->{} String
# report : Failure e ->{} Unit
# let report _ = ()
#
start : forall s. Fiber.Provided {Greeter} s => Fiber.Owner s ->{Greeter} Unit
let start owner = ignore (Fiber.spawn owner report (task { Greeter.greet "child" }))
```

Сборка `Layer.from_build f` и `Layer.from_build_memo f` выполняется во владельце
региона `s`, в который собирается слой, и `f` предполагает
`Fiber.Provided ri s` для входов слоя `ri`: зарегистрированный ею финализатор
может ими пользоваться. `provide` и `Layer.launch` собирают слой в собственный
регион, который переживает каждый поставщик его входов. `Layer.build`,
`Layer.build_with_memo_map`, `LayerRef.make`, `LayerMap.make` и
`LayerMap.from_record` собирают его в регион вызывающего, поэтому требуют там
`Fiber.Provided ri s`. Сборка в `scope` внутри `provide` одного из входов
проходит проверку; сборка в `scope` вокруг этого `provide`, который его
переживает, — это `FLICK-G003`:

```flick fail G003
# service Greeter =
#   greet : String ->{} String
# service Box =
#   value : Unit ->{} String
# greeter : Layer none none {Greeter}
# let greeter = Layer.succeed Greeter { greet = fun name -> name }
#
box : Layer {Greeter} none {Box}
let box =
  Layer.from_build (fun memo ->
    Scope.add_finalizer (task { ignore (Greeter.greet "released") })
    Layer.build_with_memo_map memo (Layer.succeed Box { value = fun () -> "box" })
  )

built : Unit ->{} Unit
let built () = provide greeter { scope { ignore (Layer.build box) } }

leaked : Unit ->{} Unit
let leaked () = scope {
  // FLICK-G003: финализатор `box` выполнялся бы при закрытии `scope`,
  // после того как `provide` освободил `Greeter`
  provide greeter { ignore (Layer.build box) }
}
```

## Что может приостановиться: `Async`

Функция, которая ожидает срабатывания таймера, завершения ввода-вывода или
другого файбера, говорит об этом эффектом `Async` в своей строке: `Effect.sleep : Duration ->{Async} Unit`,
`Database.query`, `Http.Client.get`, `Fiber.join`, `Queue.take`. Вызов со
строкой без `Async` не ожидает асинхронную работу. Обычные runtime-вызовы и
рекурсивные циклы всё равно имеют точки прерывания, включая `->{}`: пустая строка
не обещает завершения или атомарного выполнения. Чистые вызовы и встроенные
Sync-стадии UI остаются синхронными. У `Async` нет операций: его на корне обрабатывает
планировщик среды выполнения, поэтому он может быть у `main` и у задач
`spawn` и `parallel`; `provide Async` — `FLICK-E016`.

```flick
wait : Float ->{Async} Unit
foreign wait from "./timers.mjs" "wait"

twice : Unit ->{Async} Unit
let twice () =
  wait 1
  wait 2
```

`foreign`, в строке которого есть `Async`, — вызов, результат JS которого —
промис; без него — вызов, который возвращается сразу. Выполнение `foreign effect`
должно соответствовать объявленной строке: без `Async` вызов выполняется без
приостановки, а попытка приостановиться считается дефектом. Вызов, который может
приостановиться, в строке без `Async` — `FLICK-E002`; его исправление
добавляет `Async` там, где строка записана: в сигнатуре, в `Eff` или в
строке, переданной типу, как `Handlers none`.

## Явные зависимости от хоста

Код, который обращается к хосту, называет в своей строке сервис `Host`,
которым пользуется. В `Host` объявлены шесть обычных сервисов: `Host.Console`
(стандартные потоки), `Host.Clock` (чтение обычного и монотонного времени),
`Host.Random` (число в `[0, 1)`), `Host.Environment` (переменные окружения),
`Host.Arguments` (аргументы программы) и `Host.Process` (запуск дочерних
процессов). `Host.Live` — набор из всех шести, `Host.BrowserLive` — из первых
трёх. Реализаций внутри программы у них нет. Слои, которые их предоставляют, —
`Host.console`, `Host.clock`, `Host.random`, `Host.environment`,
`Host.arguments` и `Host.process`, а также `Host.live` для всех шести и
`Host.browser_live` для первых трёх — доверенные адаптеры к API хоста. Их
выбирают на входе: средство запуска или раннер `main`, раннер тестов или
`provide`.

```flick
mode : Unit ->{Host.Environment} Option String
let mode () = Host.Environment.env "MODE"

selected : Unit ->{} Option String
let selected () =
  let environment =
    Layer.succeed Host.Environment { env = fun _ -> Some "test"; vars = fun () -> [] }
  provide environment { mode () }
```

Реализации сервисов — записи: тест может перехватить вывод, зафиксировать время
и случайные числа, передать аргументы без изменения глобального состояния процесса.
Задачи читают сервисы там, где выполняются; внутренний провайдер восстанавливает
внешний после завершения.

Стандартная библиотека обращается к хосту через эти сервисы, поэтому строки её
функций называют то, чем они пользуются:

- `Platform.stdout_line`, `stderr_line` и `stdout_write`, пять уровней
  `Console` и встроенные флаги `Cli.run` — `Host.Console`;
- `Platform.args` — `Host.Arguments`;
- `Platform.env` и `Config.get`, провайдер которого по умолчанию читает
  окружение, — `Host.Environment`;
- `Clock`, `DateTime.now`, `is_past` и `is_future`, а также `Jwt` —
  `Host.Clock`;
- `Random` — `Host.Random`, поэтому подставной сервис фиксирует каждое число;
- `ChildProcess.spawn`, `string`, `lines` и `exit_code` — `Host.Process`.

`Log.console` — слой, сделанный из `Host.console`, и ему ничего не нужно:
предоставить его — значит выбрать живую консоль, как это делает вход.

### Кто их предоставляет

Каждое место, где выполняется программа, предоставляет в корне часть сервисов
`Host`:

| Где | Сервисы `Host` |
| - | - |
| Node | все шесть: `Runner.Node` предоставляет `Host.Live`, а средство запуска входа без раннера — те, что называет `main` |
| Go (`emit = "go"`) | все шесть, что бы ни называл `main` |
| `Runner.Browser`, `Runner.Cloudflare` | `Host.BrowserLive`: `Console`, `Clock` и `Random`; Cloudflare предоставляет рядом с ними свой `Env` |
| [песочница](../../play/) | те, что называет `main`, без переменных окружения и без аргументов; дочерний процесс там не запустить |
| `flick test` | `Host.Live` для `Test.effect_test`, `clock_test`, `suite` и `it` |
| эталонная модель | `Host.live` на фиксированном хосте: нет переменных окружения и аргументов, часы остановлены на 1 700 000 000 000 мс, каждое число — 0.5, процессов нет |

`Main` раннера определяет, какие сервисы может требовать `main` (см.
[точки входа](../project/#точки-входа)), а слой, который `main` предоставляет
сам, имеет приоритет над корнем. Задачи UI (`Ui.mounted`, `on_mount`,
`watched`, `watch`, обработчики `Ui.Event` и `Action`) и
`Runner.Cloudflare.wait_until` могут пользоваться `Host.BrowserLive`.
`Ui.Dom.mount` и `Ui.mount_with`, которые выполняют задачи UI с сервисами
вызывающего, требуют его, поэтому `main`, монтирующий корень, называет его.

### Где обнаруживается недостающий сервис

Тело, которому нужен сервис `Host`, не названный в сигнатуре, — ошибка
`FLICK-E002`, как и для любого сервиса. Её безопасное исправление добавляет
сервис туда, где записана строка: его предлагает редактор, а
`flick check --fix --only E002` применяет по всей программе. Чистую стрелку
или функцию обратного вызова, строку которой закрывает другой модуль, придётся
исправить вручную.

Если `main` на входе нужен сервис, которого не предоставляет ни один раннер,
это ошибка `FLICK-P026`. Таков сервис не из `Host` или сервис, которого не
называет `Main` его раннера, например `Host.Environment` при `Runner.Browser`.

### Чего не обещает пустая строка

Строка называет обращения к хосту, которые проходят через сервисы `Host`. От
кода, который писали не вы, она не изолирует: часть кода обращается к хосту,
что бы ни говорила его строка:

- привязка `foreign` принимается на доверии: `now : Unit ->{} Float`,
  привязанная к `Date.now`, читает часы при пустой строке;
- `Host.raw_*` и сделанные из них живые слои — обычные значения: любая функция
  может вызвать первые или предоставить вторые, какой бы ни была её строка:

```flick
printed_anyway : Unit ->{} Unit
let printed_anyway () =
  provide Host.console { Platform.stdout_line "no row names this" }
```

Проверяйте привязки `foreign` и использование слоёв `Host` так же, как любой
доверенный код. Есть и ещё одно ограничение: финализатор, который добавляет
`Layer.from_build`, не может пользоваться сервисом, полученным слоем, например
`Host.Console`: этот сервис предоставляет вызывающий код, и он может
освободить его раньше (`FLICK-G003`).

### Чтение времени и ожидание

`Host.Clock` читает время, но ничего не заставляет ждать. `Effect.sleep`
(`Duration ->{Async} Unit`), `delay` и расписания `Effect.retry` ждут через
планировщик, и `Host.Clock` им не нужен. Живой `Host.Clock` читает часы
файбера — те же, по которым ждёт планировщик. `Test.clock_test` заменяет эти
часы виртуальным временем: оно начинается с 0 и сдвигается только через
`Test.adjust` и `Test.set_time`, которым нужен его `VirtualTime`. В таком тесте
`sleep` ждёт виртуального времени, а `Clock.current_time_millis` и
`DateTime.now` его же и читают. Собственный `Host.Clock` меняет только то, что
возвращает чтение, — таймеры по-прежнему идут по часам файбера.

## Эффекты и handlers

`effect E params =` объявляет эффект. Его операции вызываются так же, как
операции сервиса, `E.op x`, и вызову нужен `E` в строке его стадии. Перед
именем операции можно указать, как возвращается ветка handler для неё:

- `tail`, по умолчанию: ветка — значение операции;
- `abort`: операция не возвращается, поэтому её результат — своя
  переменная, `forall b. … -> b` (`FLICK-E014`), а ветка — значение handler;
- `control`: ветка — значение handler, а `resume` — остаток тела, который
  возвращает значение handler.

Сервис — эффект, у которого все операции `tail`: другой класс в `service` —
это `FLICK-E013`. Слова `effect`, `tail`, `abort` и `control` — ключевые
только там.

`try … with` обрабатывает операции так же, как ошибки. Ветка
`| E.op p… -> h` принимает аргументы вызова; `| return p -> h` принимает
значение тела, и тогда значение handler — `h`. Ветка `return` у handler
одна, без guard, и подходит под любое значение (`FLICK-E015`); без неё
значение тела и есть значение handler. Эффект уходит из строки тела, когда
ветки без guard принимают все аргументы каждой его операции; остальные его
вызовы уходят к handler снаружи. Ветки выполняются там, где стоит handler:
то, что им нужно, берётся из охватывающей стадии, а сбой ветки `tail`
принадлежит handler, и `try` внутри тела его не видит.

```flick
effect Ask a =
  ask : Unit ->{} a

effect Emit a =
  control emit : a ->{} Unit

effect Throw =
  abort throw : forall b. String ->{} b

with_name : forall a r e. String -> (Unit ->{Ask String | r; e} a) ->{r; e} a
let with_name name body =
  try body () with
  | Ask.ask () -> name

collect : forall a r e. (Unit ->{Emit a | r; e} Unit) ->{r; e} List a
let collect body =
  try body () with
  | Emit.emit x -> [x] ++ resume ()
  | return () -> []

attempt : forall a r e. (Unit ->{Throw | r; e} a) ->{r; e} Result String a
let attempt body =
  try Ok (body ()) with
  | Throw.throw message -> Err message
```

`abort` закрывает scope, из которых выходит, до запуска своей ветки. Ветка
`control`, которая закончилась без вызова `resume`, отпускает остаток тела,
и его scope тоже закрываются; второй вызов `resume` — дефект. `resume`
держит регион handler и поэтому остаётся в ветках: сохранённый в ячейке,
созданной снаружи, он даёт `FLICK-G002`:

```flick fail G002
# effect Emit a =
#   control emit : a ->{} Unit
# zero : Unit ->{} Int
# let zero () = 0
#
escaped : (Unit ->{Emit Int; none} Int) ->{} Int
let escaped body =
  let cell = Ref.make zero
  try body () with
  | Emit.emit _ ->
    // FLICK-G002: `resume` остаётся в ветках своего handler
    Ref.set cell resume
    0
```

`provide S impl { body }` обрабатывает в теле сервис `S` записью
`impl : S.Impl`, каждую операцию её полем, как это делают ветки `tail`.
Запись передаётся по имени или в скобках: `provide S ({ op = … }) { … }`.
Эффект обрабатывают ветки `try` (`FLICK-E016`).

```flick
# service Greeter =
#   greet : String ->{} String
#
greeter : Greeter.Impl
let greeter = { greet = fun name -> "hello, \{name}" }

greeted : Unit ->{} String
let greeted () = provide Greeter greeter { Greeter.greet "ada" }
```

Задача, которую запускают `spawn`, `parallel`, гонки, `timeout` или `run`
у `FiberSet`, `FiberMap` и `FiberHandle`, выполняется в собственном файбере.
Handler эффекта с операцией `control` находится в файбере, который его
установил, поэтому такой задаче нужен `Detached r`: ни у одного эффекта её
сервисов такой операции нет. Это ограничение требуется и для работы, зарегистрированной в регионе:
финализатора, освобождения ресурса, слушателя. Такая работа выполняется при закрытии
`scope`, когда обработчик мог уже прекратить выполнение охваченного им блока и больше не может обрабатывать операции.
Компилятор выводит `Detached r` из строки; функция, которая запускает или
оставляет такую работу с сервисами переменной строки, называет его в своей
сигнатуре. Эффект `control` там или переменная строки без него — это
`FLICK-E017`:

```flick
# effect Emit a =
#   control emit : a ->{} Unit
# collect : forall a r e. (Unit ->{Emit a | r; e} Unit) ->{r; e} List a
# let collect body =
#   try body () with
#   | Emit.emit x -> [x] ++ resume ()
#   | return () -> []
#
both_of : forall r e a. Detached r => Eff r e a -> Eff r e a ->{r; e} (a, a)
let both_of first second = parallel first second

children : Unit ->{} (List Int, List Int)
let children () =
  parallel (task { collect (fun () -> Emit.emit 1) }) (task { collect (fun () -> Emit.emit 2) })
```

Ветка `tail` выполняется в стеке вызывающего кода, который может относиться к
Sync-стадии, поэтому вызовы внутри ветки не должны приостанавливаться: эффект с операцией
`control`, `Async` или переменная строки там — это `FLICK-E018`, если только
их не объявляет строка операции или не ловит handler внутри ветки.

Handlers работают на обеих целях. Ветке `control` нужно, чтобы тело ждало
как корутина: её выполняют Go и собственный рантайм Flick на JavaScript
(`effect = "rt"`), а адаптер Effect (`effect = "effect"`) её не выполняет и
сообщает `FLICK-P012`.

## Пока не проверяется

| Конструкция | Сейчас |
| - | - |
| `Layer.merge`, использованный без применения к обоим слоям | конфликты не выявляются |
| имя сервиса как значение с квалификатором модуля | не принимается; укажите имя без квалификатора |
