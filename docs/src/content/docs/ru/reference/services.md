---
title: "Сервисы, слои и хост"
source_hash: f8250197077ab1df
---

Сервис — набор операций, реализацию которых программа находит там, где
выполняется; слой строит сервисы, а `provide` передаёт их блоку (см.
[Ошибки, сервисы, ресурсы и конкурентность](../../spec/05-errors-and-concurrency/)).
Как проверяются строки вызова, описано в разделе [Эффекты и ошибки](../effects/).

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
  [любой строки](../effects/#требования-вызовов-и-границы-сигнатуры): функция с `{...Reads}` проходит там,
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
Вычитание имени — единственный механизм исключения из открытых строк во Flick
([E15, #438](https://github.com/bondiano/flick-lang/issues/438), закрывающий
вопросы открытых строк из [#188](https://github.com/bondiano/flick-lang/issues/188),
[#306](https://github.com/bondiano/flick-lang/issues/306),
[#352](https://github.com/bondiano/flick-lang/issues/352) и
[#353](https://github.com/bondiano/flick-lang/issues/353)): язык остаётся на
вычитании по имени (`{-S | r}`, `{; -E | e}`), сигнатурах с объединением строк
(`o + r`) и исключении параметризованных экземпляров (`FLICK-E019`) без
добавления новых ограничений на открытые строки. Этого достаточно для снятия
конкретных сервисов и ошибок с открытой строки в `provide` и `try`, обобщённых
комбинаторов `Eff (o + r)` и корректной обработки параметризованных сервисов.
Две границы библиотечных абстракций осознанно остаются вне системы:

- **Вычитание переменной строки (`r - ro`).** Первоклассная `Layer.run` имеет
  тип `Layer ri e ro -> (Unit ->{ro + r; e2} a) ->{ri + r; e + e2} a` и потому
  не может типизировать остаточные сигнатуры вроде
  `Layer none none {Log} -> Eff s none a ->{-Log | s} a` (`FLICK-E002`), что
  является одной из причин сохранения встроенного `provide`
  ([E12, #434](https://github.com/bondiano/flick-lang/issues/434)).
- **Абстрактная непересекаемость для обобщённых обёрток над `Layer.merge`.**
  `Layer.merge` отклоняет пересекающиеся конкретные выходы (`FLICK-E004`), но
  обобщённые обёртки, полиморфные по `o1` и `o2`, не несут абстрактного
  предиката непересекаемости.

Обработчик находит сервис по имени, а не по аргументам. `provide` слоя или записи
`Store Int` отвечает на каждую операцию `Store` в своём теле, поэтому телу может
быть нужна переменная строки, только если она не держит другого экземпляра
`Store`: из неё он убран, `Eff {-Store | r} …`, или экземпляр назван рядом с ней,
`Eff {Store Int | r} …`, и тогда `Store` в ней только `Store Int`. Переменная,
которая может держать `Store String`, получила бы вместо него `Store Int`
обработчика; это `FLICK-E019`, как и функция с такой переменной там, где
ожидается функция с `{Store Int | r}`. Handler эффекта с аргументами работает
так же ([Эффекты и handlers](../effects/#эффекты-и-handlers)).

У набора могут быть параметры, как в `type alias Db s = {Scope s, Log}`.

Набор расширяет сигнатуру: вызывающий код должен предоставить его целиком, и тесты
тоже. Используйте наборы на границах модулей, где многим функциям нужно одно и то же.
Маленькой функции оставьте точную строку или дайте её вывести.
Пример, который можно запустить, есть в [уроке тура](../../tour/sets/).

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

Обе формы — `provide layer { body }` и `provide S impl { body }` — остаются
встроенными конструкциями языка, а не переносятся в `Layer.run` или плагин
синтаксиса ([E12, #434](https://github.com/bondiano/flick-lang/issues/434)):
понижение `provide layer { body }` в `Layer.run layer (fun () -> body)` не
означает эквивалентности исходной типизации. Граница замыкания `Layer.run` не
сохраняет присваивания внешней `let mut` (`FLICK-G005`), лексические
`break`/`continue` цикла (`FLICK-T071`) и лексический `defer`/`errdefer`
(`FLICK-G004`), не может вычесть сервис из абстрактной переменной строки
(`{-Log | s}`, `FLICK-E002`), а `provide S impl` напрямую устанавливает запись
реализации без построения слоя. Прямые вызовы `Layer.run` выполняют те же
проверки безопасности, что и `provide`: время жизни провайдера
(`Fiber.Provided`, `FLICK-G003`), исключение параметризованных экземпляров
(`FLICK-E019`) и проверку живости региона при установке индексированных регионом
сервисов (`FLICK-G002`).

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
`provide`. Пример, который можно запустить, есть в [уроке тура](../../tour/host/).

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
| Go (`[build] target = "go"`) | все шесть, что бы ни называл `main` |
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
`FLICK-P026` требует точки входа: профиль поддержки каждой цели, который
сообщал бы в библиотеке и в редакторе, чего не может предоставить окружение,
— дальнейшая работа ([T2, #445](https://github.com/bondiano/flick-lang/issues/445)).

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
доверенный код.

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

## Пока не проверяется

| Конструкция | Сейчас |
| - | - |
| `Layer.merge`, использованный без применения к обоим слоям | конфликты не выявляются |
