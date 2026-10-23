---
title: "7. Ошибки сервисы ресурсы и конкурентность"
---

## Ожидаемые ошибки

Ошибки имеют номинальную идентичность и payload. `try ... with` обрабатывает выбранную ожидаемую ошибку по нативной семантике Effect. В первой версии payload каждого error-конструктора обязан удовлетворять `NoCapture`: в нём нельзя сохранить scoped-handle, захватывающий регион callback/task, ячейку с таким значением или скрыть capability через existential/Unknown. Проверка действует даже для локально перехватываемой ошибки. Цена ограничения — в ошибку передают путь, id или копию данных, а не живой ресурс.

Ошибка объявляется `type error`. Её единственный constructor называется так же, как сама ошибка, и перечисляет поля как вариант ADT: `type error NotFound = NotFound UserId`. Ошибка без полей пишется `type error Timeout`.

```flick
type error NotFound = NotFound UserId

load_optional : UserId ->{Users; none} Option User
let load_optional id =
  try
    Some (Users.get id)
  with
  | NotFound _ -> None
```

При типе `Users.get : UserId ->{Users; NotFound} User` результатом `load_optional` становится `Option User`, требованиями остаются `{Users}`, а строка выбранных recoverable ошибок становится пустой. Полный native Cause может хранить дополнительные диагностические failures; его payloads не выдаются за остаточную `e`.

Ошибка может иметь параметры. `type error RetryExhausted e = RetryExhausted e Number` хранит последнюю причину после исчерпания попыток, `type error SendFailed a = SendFailed a` возвращает значение, которое не удалось отправить. Строка ошибок различает элементы по номинальному имени `Module.Name`, а не по аргументам, поэтому в одной строке не больше одного элемента с данным именем: аргументы двух `RetryExhausted` одной строки унифицируются, и `RetryExhausted DbError` вместе с `RetryExhausted String` — ошибка типов. TS-проекция устроена так же: `_tag` не зависит от аргументов, `RetryExhausted<E>` встречается в объединении ошибок `Effect` не более одного раза, и `catchTag` сужает тип однозначно. Java и OCaml по той же причине запрещают параметризованные исключения; здесь параметры допустимы, потому что строка хранит один элемент на имя.

```flick
type error RetryExhausted e = RetryExhausted e Number

retry : Number -> Eff r e a ->{r; RetryExhausted (Errors e)} a
```

Удаление ошибки из строки разрешено только при полном покрытии её номинального варианта в выбранном typed recovery. Guard или частичный pattern по payload оставляет ошибку возможной. Requirements/errors обработчика добавляются к результату. Вычитание строки относится к выбранной recoverable ошибке, а не доказывает отсутствие такого tag среди вторичных raw failures.

Expected failure, defect и interruption остаются различными исходами. Пустая строка ошибок не гарантирует завершение, отсутствие дефекта или невозможность отмены. В Effect defects и interruption сохраняются в runtime-причине завершения, а не в обычном параметре `E`. [Виды ошибок Effect](https://effect.website/docs/v4/error-management/two-error-types).

`try ... with` компилируется в обработку typed failure Effect. JavaScript `try/catch`, окружающий только построение значения Effect, не реализует этот контракт.

## Сервисы и классы типов

| Механизм | Как выбирается реализация | Для чего нужен |
| - | - | - |
| Класс типов | Frontend разрешает instance и передаёт dictionary | `Eq`, `Show`, `Functor` и другие операции над типами |
| Сервис | Реализация предоставляется для конкретного вычисления | База данных, HTTP, логирование, тестовое окружение |
| Layer | Runtime строит сервисы из зависимостей | Инициализация, приобретение ресурсов и составление окружения |

Чистая операция `show user` не должна требовать Effect Context. Подмена production-сервиса тестовым не должна менять глобальный выбор typeclass instance.

В прямом API `Users.get` — сгенерированная эффектная операция. На стороне реализации сервис может хранить метод, возвращающий `Effect`. Получение сервиса добавляет требование `Users`; вызов уже полученного экземпляра не обязан повторно искать его в Context.

Сервисы и ошибки получают квалифицированную идентичность пакета и модуля. Binding существующего сервиса/ошибки Effect сохраняет его реальный key, tag и layout. Декларация сервиса перечисляет полные сигнатуры методов; compiler генерирует operations и тип implementation-record:

```flick
service Clock =
  now : Unit ->{} Number

clock_impl : Clock.Impl
let clock_impl =
  { now = JsClock.now }

clock_layer : Layer none none {Clock}
let clock_layer =
  Layer.succeed Clock clock_impl
```

Здесь `JsClock.now` — явно объявленный runtime-binding. `Clock.now` требует сервис `Clock`; метод полученного `Clock.Impl` не делает повторный lookup. Скрытые captures implementation-record сохраняются в его типе и Layer. Service methods не становятся class instances. [Сервисы Effect](https://effect.website/docs/v4/requirements-management/services).

## Предоставление окружения

Порядок параметров слоя — `Layer inputs errors outputs`. Например:

```text
UsersLive :
  Layer {Config} {ConnectError} {Users}
```

Блок `provide layer { ... }` устанавливает окружение до выполнения тела:

```flick
application : UserId ->{Config, Log; NotFound, ConnectError} String
let application id =
  provide UsersLive {
    load_name id
  }
```

Если `load_name` требует `{Users, Log}` и допускает `{NotFound}`, после предоставления `UsersLive` остаются требования `{Config, Log}`, ошибки `{NotFound, ConnectError}` и тот же результат `String`.

Предоставление слоя убирает обеспеченные сервисы, но добавляет зависимости и ошибки его построения. Для обычной функции вместо специального блока аргументом должен быть `task { ... }`, чтобы строгий порядок вычисления не запустил тело раньше установки окружения. [Layers Effect](https://effect.website/docs/v4/requirements-management/layers).

## Ресурсы

`scope` сочетает runtime-финализацию и статический запрет небезопасного escape. Приобретение регистрирует finalizer у лексического владельца. В безопасном scoped API **нет раннего close**; отдельные manual-handle bindings не получают эту гарантию. Такое ограничение позволяет начать без линейности и typestate открытого/закрытого handle.

Выбраны свежие regions и транзитивный учёт captures. Через безопасный API нельзя получить пригодную к использованию capability уже завершённого региона. Native Cause может физически удерживать закрытый foreign объект; безопасная оболочка не раскрывает его. Полная affine/linear система не входит в первую версию. Native [Effect Scope](https://effect.website/docs/v4/resource-management/scope) выполняет финализацию; статическая часть и двухфазный owner принадлежат compiler/runtime-adapter Flick.

Scoped finalizer имеет пустой канал expected errors; его сбой остаётся defect в полном исходе. Лексический `bracket` допускает отдельную typed ошибку release и добавляет её к error row. Эти операции различаются явно. Первичная ошибка и cleanup сохраняются по нативному контракту Cause; release не заменяется обычным JS `finally` с потерей исходного сбоя.

## Конкурентность

Основной API — structured `parallel` и `parallel_all` над холодными `Eff`. Они отменяют активные ветки при failure и ждут их завершения. `parallel_all` требует конечный положительный целый предел конкурентности; результаты успеха следуют порядку входа.

Несколько наблюдённых сбоев сохраняются в нативном Cause; API не обещает собрать все будущие ошибки. `parallel_results` отдельно накапливает выбранные typed errors как `Result`, `parallel_exits` — непрозрачные полные исходы. Defects и внешняя interruption не маскируются обычным успешным списком.

`spawn` требует явных lexical owner и supervisor; `join` ожидает конкретный fiber и восстанавливает его failure. Child errors не добавляются в error row родителя автоматически, поэтому supervisor обязателен даже если handle может быть проигнорирован. В безопасном API нет неограниченного daemon fork с локальными captures. [Fibers Effect](https://effect.website/docs/v4/concurrency/fibers).

## Граница языкового ядра

Retry, schedules, streams, queues, cache, tracing и HTTP остаются типизированной стандартной библиотекой. Для повторного запуска и конкурентности она принимает описания вычислений. Специальных конструкций синтаксиса для них в первой версии нет.

Полноценный `deriving Schema` и codecs для внешних форматов входят в целевой язык. Generics, рекурсия, refinements, policies полей и effectful transformations разобраны в разделе 9.

Сервисы и `provide` не означают универсальные algebraic handlers с захватом и многократным возобновлением continuation. В первом срезе операции языка опираются на конкретные механизмы Effect; универсальные handlers не входят в этот контракт.

## Контракты исполнения и статического lifetime

Базовые правила ниже описывают поведение программы, которое должно оставаться одинаковым при смене lowering. Более сложные режимы можно добавлять отдельными библиотечными операциями.

| Область | Контракт первой версии | Цена |
| - | - | - |
| Typed errors | Номинальные, region-free payloads; `e` — выбранная recoverable ошибка | Нельзя считать все raw Fail значениями `Errors e` |
| Defects и interruption | Сохраняются вне `e`; доступны через opaque Failure/Exit | Обычный handler не является универсальным catch |
| Provide | Сначала построение Layer, затем тело; явный override конфликтов | Нет автоматического выбора неоднозначных сервисов |
| Sharing | Одна Layer identity разделяется внутри одной сборки | Между provide нужна явная owned memoization |
| Scope | Fresh regions/captures; сначала завершение children, затем LIFO resource cleanup | Закрытие может ждать uninterruptible или foreign работу |
| Cleanup errors | Scoped finalizer без expected error; typed release через bracket | Ошибки всех динамических finalizers не объединяются в e |
| Parallel | Fail-fast, отмена активных веток, ожидание cleanup, входной порядок результатов | Не гарантирует все потенциальные failures |
| Spawn | Явный owner и обязательный supervisor; join восстанавливает failure | Успех родителя не означает успех фоновых задач |
| Race/timeout | Нативный выбор победителя и ожидание losers | Нет жёсткого deadline и автоматического добавления loser failures |

## Ошибка как данные и ошибка выполнения

Чистая функция может вернуть `Result`, не запуская Effect. Исходный порядок `Result error value` удобен для HKT; runtime-представление — нативный `Result.Result<A, E>` из [Effect v4](https://effect.website/docs/v4/data-types/result), с перестановкой параметров в TS-проекции. Параметр `error` здесь имеет kind `Type`; строка runtime-ошибок имеет kind `ErrorRow`. Это разные позиции.

```text
parse_config :
  String -> Result ParseError Config

fetch_config :
  Url ->{Http; NetworkError} Config
```

Перевод `Result` в typed failure — явная операция библиотеки:

```text
from_result : Result (Errors e) a ->{; e} a
```

Для `Result NotFound User` используется singleton-строка `{NotFound}`. `Result String a` требует сначала преобразовать строку в объявленную номинальную ошибку. Операция не перехватывает defects или interruption. Обычный `try ... with` обрабатывает expected failures, отдельный API `Exit`/`Cause` — полный исход. [Разделение ошибок Effect](https://effect.website/docs/v4/error-management/two-error-types).

```flick
type error NotFound = NotFound UserId
type error DbUnavailable = DbUnavailable String

find_user :
  UserId ->{Users; NotFound, DbUnavailable} User

find_optional :
  UserId ->{Users; DbUnavailable} Option User
let find_optional id =
  try
    Some (find_user id)
  with
  | NotFound _ -> None
```

### Сложный Cause: что обещает строка ошибок

В принятом контракте `e` описывает **ошибку, доступную обычному typed recovery**. Полный native Cause может содержать вторичные payloads вне остаточной `e`; safe API не предоставляет `Failure e -> List (Errors e)` для всех его Fail.

Проверенный ориентир — Effect 4.0.5, commit `81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f`. В v4 Cause содержит плоский список `Fail`, `Die`, `Interrupt`; дерево последовательных/параллельных причин не является контрактом. [Cause](https://effect.website/docs/v4/data-types/cause).

| Причина | Нативное typed recovery в проверенном срезе |
| - | - |
| Один `Fail(A)` | Выбирается A |
| `Fail(A), Fail(B)` | Выбирается первый Fail; handler не обходится по всему списку |
| `Fail(A), Interrupt` | Сам Interrupt в списке не блокирует выбор A |
| `Fail(A), Die(D)` | Обычное typed recovery блокируется; Fail переводятся в defects |
| Только Interrupt | Обычный typed handler не получает ошибку |
| `Fail(B), Fail(A)`, handler только A | Handler не вызывается; распространяется исходная причина |

Последняя строка существенна: нативный `catchTag(A)` уменьшает E в TS-сигнатуре, хотя вторичный A может остаться в полном Cause. Поэтому нельзя одновременно механически наследовать этот handler и выдавать каждый raw `Fail` за `Errors e`. Проверены [выбор recoverable error](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/internal/effect.ts#L160-L199) и [catchIf/catchTag](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/internal/effect.ts#L2947-L3068).

### Выбранный контракт Cause и цена альтернатив

| Модель | Гарантия | Цена |
| - | - | - |
| Нативное recovery и opaque RawCause — выбрано | Handler соответствует выбранной recoverable ошибке; rethrow сохраняет native Cause | Safe diagnostics возвращают инертный отчёт, а не исходные JS payloads |
| Нормализация остаточной причины — вне первой версии | Все активные Fail принадлежат остаточной e; вторичные исключённые failures хранятся отдельно | Собственный adapter каждого сужающего handler, обход причин и отличие от native catchCause |
| Полностью типизированный Cause — вне первой версии | Все Fail имеют тип Errors e | Частичный handler часто не может уменьшить e; нужны дополнительные доказательства |

`Exit e a`, `Failure e` и `RawCause` — opaque типы. Внутри сохраняется нативное представление Effect; типизированный и диагностический интерфейсы разделены.

```text
Failure.primary :
  Failure e -> Option (Errors e)

Failure.raw_cause :
  Failure e -> RawCause

Failure.inspect :
  Failure e ->{} CauseReport

RawCause.inspect :
  RawCause ->{} CauseReport

Failure.rethrow :
  Failure e ->{; e} a
```

Успешный `Exit` содержит `a` и сохраняет captures этого `a`. `Failure.primary` выдаёт только выбранную typed error; `rethrow` восстанавливает исходный Cause с той же `e` либо с её допустимым расширением. Безопасного `RawCause -> Exit e a` для произвольной `e` нет. Операции не могут стирать известные captures. Typed error payloads проверены как region-free; произвольные foreign defects остаются запечатанными.

Safe `RawCause` **не раскрывает исходный `JsValue`**. `CauseReport` содержит инертные виды причин, безопасно полученные tags/текст и interruption metadata. Инспекция не вызывает пользовательские getters, `toString` или `toJSON`; неподдерживаемый payload отображается placeholder. Доступ к исходному JS-графу возможен только через явный unsafe FFI. Bindings native Cause-transformers проверяются отдельно: нельзя приписать остаточную `e` каждому вторичному Fail или выполнить callback для него под ложным типом.

Во втором варианте `[Fail(B), Fail(A)]` после handler A становится активным `Fail(B)` с исходным A в диагностике. Переводить этот A в `Die` нельзя: это изменит последующее typed recovery. В третьем варианте правило «сложную причину просто пропускаем, но A всё равно вычитаем» тоже некорректно.

Дополнительный вариант для `parallel` — явный `CompositeError`. Он меняет прикладной API: обработка `NotFound` не равна обработке агрегата с NotFound внутри. Такой тип полезен как отдельный combinator, но не обязателен для всех вычислений.

Успешное recovery может поглотить причину и её вторичные diagnostics. Если они нужны в логах, наблюдение выполняют до recovery. Правило выбора ошибки из списка не означает, что handler может произвольно отключить уже запрошенную отмену fiber: состояние interruption принадлежит runtime.

## Lifetime и sharing при provide

`provide` сначала полностью строит Layer и лишь затем выполняет тело. При ошибке построения тело не запускается. Scoped реализации принадлежат lexical owner provide; после выхода восстанавливается внешнее окружение и выполняется двухфазное закрытие. Captures сервиса, implementation-record, closure, `Eff`, UI или codec сохраняются в интерфейсах и не могут пережить owner.

`task { service_call () }` ищет требуемый сервис при запуске в текущем окружении. Если задача уже захватила конкретный экземпляр сервиса, она сохраняет его lifetime. Finalizer получает контекст, сохранённый при регистрации, а не случайный сервис из окружения более позднего закрытия.

Одна Layer identity переиспользуется внутри одной сборки графа. Независимые `provide` строят независимые экземпляры; глобального singleton нет. Долгое разделяемое построение требует явной memoization с owner. Две реализации одного service key в графе — ошибка, кроме явного override combinator. Это правило опирается на [memoization слоёв Effect](https://effect.website/docs/v4/requirements-management/layer-memoization), а исходный checker дополнительно проверяет lifetime.

## Финализация и границы ресурса

```flick
read_config :
  Path ->{FileSystem; OpenError, ReadError} String
let read_config path =
  scope {
    let file = File.open_scoped path
    File.read_all file
  }
```

`File.open_scoped` требует лексический `Scope s`, который предоставляет `scope`. После успешного acquisition регистрируется finalizer. Итог возвращается после завершения принадлежащих области детей и последующего resource cleanup. Ресурсные finalizers выполняются LIFO. Один смешанный LIFO-список children и ресурсов этого не обеспечивает, если ресурс зарегистрирован после запуска ребёнка. [Scope API](https://effect.website/docs/v4/api/effect/Scope).

Scoped `acquireRelease` требует release без expected errors; лексический `bracket` на базе `acquireUseRelease` допускает typed ошибку release. Именно это разделение принято в языке. Cleanup defects сохраняются в полном исходе; успешное закрытие каждого внешнего ресурса не является теоремой checker. [Нативные операции Effect](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/Effect.ts).

Для безопасных scoped API checker проверяет прямой и косвенный escape: результат, closures, task, контейнеры, existential-обёртки, запись во внешнее состояние и передачу удерживающему FFI. Ресурс может использоваться многократно внутри своей области, если его API это разрешает. Статический lifetime сам по себе не доказывает отсутствие гонок или безопасность конкурентного использования handle.

### Статические regions и захваты

Каждый `scope s { ... }` создаёт fresh region `s` и runtime owner. В `scope { ... }` имя выводится и остаётся скрытым. Scoped-handle имеет тип вроде `File s`. Owner передаётся compiler лексически; он не реализуется одним обычным Context lookup, который во вложенной области мог бы выбрать другой scope.

```flick
bad_deferred :
  Path ->{FileSystem; OpenError} Eff none {ReadError} String
let bad_deferred path =
  scope {
    let file = File.open_scoped path
    task { File.read_all file }
  }
```

Это ошибка: задача захватывает `file`, а её пытаются вернуть после закрытия области. Обычный тип `Eff none {ReadError} String` не показывает эту зависимость, поэтому одной проверки `s` в типе результата недостаточно.

Корректно перенести приобретение внутрь отложенного выполнения:

```flick
deferred_read :
  Path -> Eff {FileSystem} {OpenError, ReadError} String
let deferred_read path =
  task {
    scope {
      let file = File.open_scoped path
      File.read_all file
    }
  }
```

Теперь каждый `await` получает собственную область и ресурс. Возвращаемая строка может пережить файл.

Та же проверка нужна для обычного callback:

```flick
bad_callback :
  Path ->{FileSystem; OpenError} (Unit ->{; ReadError} String)
let bad_callback path =
  scope {
    let file = File.open_scoped path
    fun () -> File.read_all file
  }
```

Такой callback также нельзя вернуть. Higher-rank `forall s` препятствует прямому выходу `File s`, но не доказывает отсутствие захвата в callback, задаче или generic-контейнере без дополнительной системы captures.

| Путь утечки | Требование checker |
| - | - |
| Прямой result или вложенный record/ADT | Регион не переживает владельца; зависимость распространяется транзитивно |
| Closure или Eff | Captures сохраняются при передаче, обобщении и через интерфейсы модулей |
| Existential/opaque упаковка | Номинальность и скрытие типа не стирают lifetime |
| Запись во внешний Ref или cache | Время жизни хранилища не превышает допустимое время жизни записываемого значения |
| Ошибка, Exit или диагностическое значение | Проверяются все выходы, включая payload failures; путь ошибки не обходит lifetime |
| Foreign вызов | Region-bound аргумент требует проверенного retention-контракта |

Captures — отдельное статическое свойство, не третья строка эффекта. Запись `T^{s, t}` задаёт конкретные regions, `T^c` — строку kind CaptureRow, `T^(c + d)` — их объединение. Явный qualifier — верхняя граница полного набора reachable usable captures значения: известные из T зависимости обязаны входить в неё. Разрешено расширение этой границы, но не небезопасное сужение. Bare T не добавляет скрытых captures и не стирает intrinsic dependencies: File s и List (File s) зависят от s. Само упоминание s во входном типе функции не означает, что её closure держит ресурс s; checker отслеживает достижимость capability, а не все имена в сигнатуре.

```text
read_later :
  forall s.
  File s -> (Eff none {ReadError} String)^{s}

scoped_callback :
  forall s.
  File s -> (Unit ->{; ReadError} String)^{s}
```

Это интерфейсные сигнатуры. Локальные captures выводятся; публичные сигнатуры и `.iface` сохраняют их. Захваты проходят через partial application, records, opaque/existential types, dictionaries и `Unknown^{s}`. Передача callback через generic higher-order API требует capture-polymorphic контракта, как у `List.map_effect`. В коротких scoped-примерах имя region опущено в синтаксисе, но сохраняется checker. Mutable storage инвариантно; более долгоживущий Ref/cache не может получить более короткую capability.

Проверка захватов в [Scala](https://docs.scala-lang.org/scala3/reference/experimental/cc.html) даёт полезный ориентир для closures и контейнеров; это не готовая реализация checker Flick.

### Объём сильного контракта

| Свойство | Контракт первой версии |
| - | - |
| Многократное использование handle в живой области | Разрешено, если API ресурса допускает это |
| Раннее close | Отсутствует в безопасном scoped API |
| Полная линейность | Не вводится; lifetime проверяется через regions/captures |
| Конкурентный доступ | Binding объявляет share-safe операции либо сериализует доступ; lifetime не доказывает отсутствие гонок |
| Cleanup | Effect выполняет finalizers; compiler не доказывает физический успех внешнего close |
| Typed errors | Payload region-free даже при локальном recovery |
| Native Cause | Живые foreign payloads запечатаны; безопасно доступен только inert report |
| Непроверенный JS | Требует явного trusted/unsafe контракта |

### Владелец fibers и порядок освобождения

Задача может захватывать ресурсы своей области и более долгоживущих областей. Перенос в более долгоживущего владельца допустим только при отсутствии коротких захватов. Подписка или callback, удерживаемый внешней библиотекой, также имеет проверяемого владельца.

Закрытие strong scope имеет обязательный порядок: **запретить новую работу → запросить отмену детей → дождаться терминального состояния каждого → выполнить resource finalizers LIFO → вернуть итоговый Exit**. Если ребёнок не завершает uninterruptible участок, закрытие ждёт. Освобождать используемый ресурс под продолжающим работу ребёнком запрещено; timeout не отменяет это правило.

Терминальное состояние Effect-fiber ещё не доказывает остановку внешней Promise-операции. Async FFI со scoped captures обязан перед завершением отмены дождаться подтверждения остановки либо фактического окончания внешней работы. Один `AbortSignal` недостаточен. Если операция не отменяется, adapter ждёт её окончания; detach допустим лишь без коротких captures либо при явном безопасном переносе владения.

Простое смешивание `forkScoped` и resource finalizers в одном LIFO-списке не гарантирует этот порядок при любых регистрациях. Backend должен иметь отдельную группу детей или эквивалентное разбиение на scopes. Это adapter поверх Effect.

Nursery в первую версию не входит. Structured combinators владеют своими ветками; scoped background fiber отменяется при закрытии owner. Ошибки background work всегда наблюдает явно переданный supervisor, а `join` дополнительно восстанавливает failure для ожидающего. Это не означает автоматическое объединение child errors с error row тела scope.

## Конкурентные операции с разными контрактами

```flick
load_pair : UserId -> UserId ->{Users; NotFound} (User, User)
let load_pair first_id second_id =
  parallel
    (task { Users.get first_id })
    (task { Users.get second_id })
```

Для `parallel` порядок результатов совпадает с порядком аргументов. Неуспех прекращает запуск следующей работы, запрашивает отмену остальных активных веток и ожидает их завершения. В проверенной реализации Effect при этом собираются наблюдённые не-Interrupt причины, включая некоторые сбои останавливаемых веток; не обещается получение всех возможных будущих ошибок.

Операция не закрывает внешний `Scope`: зарегистрированные в нём ресурсы живут до его закрытия. Если ресурс должен закрыться вместе с веткой, ветка создаёт собственный `scope`. Защищённый cleanup может задержать завершение. Для `parallel_all` обязателен конечный положительный целый лимит конкурентности, а результаты сохраняют порядок входа. [Реализация конкурентного обхода Effect](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/internal/effect.ts#L5098-L5252).

`parallel_results` переводит выбранные expected failures в `Result` и позволяет дождаться остальных ветвей. Defects и внешняя interruption остаются управляющими событиями. `parallel_exits` возвращает opaque полные исходы завершённых ветвей; внешняя отмена группы всё равно распространяется. Safe API не обещает собрать все потенциальные ошибки fail-fast операции.

Привязка к scope задаёт lifetime; `join` — ожидание конкретного результата. В текущем [Fiber API](https://effect.website/docs/v4/api/effect/Fiber#joinAll) `joinAll` при ошибке прекращает ожидание, но сам не прерывает остальные fibers. Поэтому backend реализует structured combinators через отдельную child group с отменой и join всех активных ветвей, а не одним вызовом joinAll.

### Конкурентность: три модели

| Модель | Завершение и ошибки | Цена |
| - | - | - |
| Structured combinators + явные scoped fibers — выбрано | `parallel` владеет ветками; фоновые задачи запускаются отдельной операцией, результат получают через обязательный supervisor и, при необходимости, join | Для динамических групп нужно явно передавать управление группой |
| Языковая nursery | Нормальный выход ожидает детей; ошибка тела/ребёнка отменяет связанных участников; ошибки детей учитываются даже без join | Реестр детей, правила объединения причин и учёт error row детей в типе nursery |
| Scoped background supervision | Закрытие scope отменяет фоновые задачи; ошибки получает supervisor, лог или канал | Успех родителя не означает успех всех фоновых задач; нужна явная политика наблюдения |

В первой версии используются structured combinators и scoped supervision. `spawn owner supervisor task` регистрирует ребёнка у owner; task и supervisor не могут захватывать более короткий регион. Потеря Fiber handle не теряет ошибку: supervisor получает полный `Failure`, различая expected shutdown interruption. Defect supervisor передаётся root runtime reporter. Для UI дополнительно действует правило завершения root при сбое его supervisor. Долгоживущая работа требует явно более долгого owner; безопасного daemon fork с короткими captures нет.

Scoped lifetime и ожидание естественного успеха детей различаются. Обычный scoped fiber останавливается при закрытии scope; ошибка не присоединяется автоматически к результату тела. В nursery такое присоединение — дополнительный обещанный контракт. Higher-rank callback помогает ограничить capability области, но сам по себе не учитывает ошибки детей и не доказывает отсутствие escape ресурсов.

### Race и timeout

| Контракт | Поведение | Цена |
| - | - | - |
| Нативные операции — выбрано | `race_success`: первый успех; `race_first`: первое завершение; losers прерываются и ожидаются; выбранный результат сохраняется | Их Exit не добавляются автоматически к результату; cleanup defect проигравшего может не стать ошибкой победителя |
| `race_checked` вне первой версии | После выбора результат проверяется по исходам остановки losers | Победитель может быть отвергнут; нужно различать ранние failures и сбои cleanup |
| Ранний timeout вне первой версии | Ответ возвращается после выбора таймера, завершение работы передаётся более долгому владельцу | Работа живёт после ответа; обязательны scope, supervisor и правила дальнейших ошибок |

В проверенном [Effect race](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/internal/effect.ts#L1649-L1739) ожидание проигравших не добавляет их Exit к выбранному результату. Поэтому контракт `parallel` нельзя автоматически переносить на гонку.

`timeout` и `timeout_option` используют нативный выбор между вычислением и таймером, затем прерывают и ожидают losers. `timeout` добавляет `TimeoutError`; `timeout_option` представляет именно таймаут через `None`, сохраняя другие failures. Жёсткой wall-clock гарантии нет: синхронный JS, uninterruptible участок и cleanup могут задержать возврат. `race_checked` и ранний возврат с ещё живыми scoped children в первую версию не входят. Не гарантируется старт всех ветвей при синхронном победителе или стабильный winner при одновременной готовности.

**Runtime первой версии:** нативное typed recovery, opaque Failure/Exit/RawCause и inert diagnostics; fresh regions/captures; двухфазный scope с обязательным ожиданием foreign quiescence; structured combinators и scoped supervision; native race/timeout. Допустимые оптимизации обязаны сохранять эти границы, исходный Cause при rethrow, контекст сервисов и выполнение finalizers.
