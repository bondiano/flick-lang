---
title: "8. Генерация TypeScript и граница FFI"
---

## Чистые функции

```flick
identity : a -> a
let identity value =
  value

greet : String -> String -> String
let greet prefix name =
  prefix ++ ", " ++ name

hello : String -> String
let hello =
  greet "Hello"
```

```ts
export const identity = <A>(value: A): A =>
  value

export const greet =
  (prefix: string) =>
  (name: string): string =>
    prefix + ", " + name

const hello = greet("Hello")
```

Чистые функции получают обычное JS/TS-представление. Каррирование сначала сохраняется буквально; оптимизация полного применения не должна менять порядок и момент вычисления аргументов.

## ADT и pattern matching

```flick
with_default : a -> Option a -> a
let with_default fallback value =
  match value with
  | None -> fallback
  | Some x -> x
```

```ts
import { Option } from "effect"

const withDefault =
  <A>(fallback: A) =>
  (option: Option.Option<A>): A => {
    switch (option._tag) {
      case "None":
        return fallback
      case "Some":
        return option.value
    }
  }
```

В этом примере `Option` — стандартный тип с нативным представлением Effect. Пользовательский ADT с похожим именем или тегами остаётся другим типом. Исходный frontend обеспечивает исчерпываемость и номинальную идентичность; `readonly` в TS описывает внешний интерфейс, а гарантия неизменяемости также зависит от FFI и доступных примитивов.

### Варианты, guards и or-patterns

Constructor несёт ноль, одно или несколько полей. Поля перечисляются после имени constructor так же, как аргументы при его применении и в pattern: `| Circle Point Number`. Поле — атомарный тип, поэтому применение типа и стрелку берут в скобки: `| Branch (List (Tree a))`, `| Handler (a -> b)`. `Circle (Point, Number)` — одно поле-tuple, а не два поля. В ABI поля занимают `_0`, `_1` и далее в порядке объявления.

Правая часть `type` всегда перечисляет варианты, ведущий `|` необязателен: `type Wrapper = Wrap Number` — ADT с одним constructor. Прозрачный alias, в том числе record-тип, объявляется `type alias`: `type alias UserId = Number`. `opaque type alias` скрывает представление за пределами модуля, `opaque type` с вариантами скрывает constructors. `alias` — ключевое слово только сразу после `type`.

```flick
type Shape =
  | Circle Point Number
  | Rect Number Number
  | Empty

area : Shape -> Number
let area shape =
  match shape with
  | Circle _ radius -> Number.pi * radius ** 2
  | Rect width height when width == height -> width ** 2
  | Rect width height -> width * height
  | Empty -> 0

is_round : Shape -> Bool
let is_round shape =
  match shape with
  | Circle _ _ | Empty -> true
  | Rect _ _ -> false
```

Ветка `match` или `try` может иметь guard `when e`, а паттерн — альтернативы `p | q`. Ограничения следуют Rust:

| Правило | Где проверяется |
| - | - |
| Все альтернативы `p \| q` связывают одинаковые имена одинаковых типов | Имена — parser (`FLICK-S012`), типы — checker |
| Одно имя связывается паттерном или параметрами одной функции не более одного раза | Parser (`FLICK-S012`) |
| Or-pattern допустим в ветке `match`/`try` и вложенно: в скобках, полях record и элементах List | Грамматика |
| Guard относится ко всей ветке, включая все её альтернативы, видит имена её паттерна и не связывает новых | Грамматика |
| Guard — чистое выражение типа `Bool`; runtime-вызов в guard запрещён, иначе порядок проверки веток стал бы наблюдаемым | Checker |
| Ветка с guard не учитывается в доказательстве исчерпываемости | Checker |

Ветки проверяются сверху вниз, альтернативы — слева направо; выполняется первая подошедшая ветка, guard которой истинен.

## Эффектная функция

```flick
load_name : UserId ->{Users, Log; NotFound} String
let load_name id =
  let user = Users.get id
  let name = normalize_name user.name

  Log.info name

  name
```

```ts
export const loadName = (
  id: UserId
): Effect.Effect<string, NotFound, Users | Log> =>
  Effect.gen(function* () {
    const user = yield* getUser(id)
    const name = normalizeName(user.name)

    yield* logInfo(name)

    return name
  })
```

`getUser` и `logInfo` здесь — сгенерированные accessors. Backend вставляет `yield*` на эффектных вызовах и оставляет чистые вызовы обычными. Генераторы являются одним из способов композиции Effect, а не требованием к исходному синтаксису. [Effect.gen](https://effect.website/docs/v4/getting-started/using-generators).

## HKT на границе TypeScript

HKT и исходные kinds проверяет frontend; при генерации JS они стираются либо остаются в передаваемых dictionaries. Публичный TS API первой версии экспортирует конкретные применения HKT и представимые dictionary-records. Абстрактный higher-kinded export, который TS не выражает верно, требует явного конкретного façade; importer/exporter не изображает его через ложный `F<A>` или `any`.

Нативная запись `F<A>` для произвольного параметра типа `F` не является возможностью TypeScript. Успешная проверка TS-проекции поэтому не заменяет проверку исходной программы. [TypeScript для функциональных программистов](https://www.typescriptlang.org/docs/handbook/typescript-in-5-minutes-func.html).

## FFI

| Категория | Контракт |
| - | - |
| Pure binding | Явное доверенное утверждение о чистоте вызова и инициализации модуля |
| Sync operation | Откладывается до запуска; исключения переводятся только по явному mapping |
| Async operation | Promise создаётся при запуске; policy определяет cancellation и ожидание quiescence |
| Effect-producing function | `foreign effect` откладывает вызов JS-фабрики и выполняет её результат в текущем fiber |
| Готовое Effect value | Импортируется как холодный Eff; эффектная загрузка модуля остаётся runtime |
| Внешние данные | Произвольный input — JsValue; source records/ADT требуют checked adapter |

Предварительное создание Promise нарушает холодность `task`. Если внешняя работа использует scoped capability, adapter обязан при отмене дождаться подтверждённой остановки или завершения underlying operation. Сам `AbortSignal` и завершение прерванного fiber этого не доказывают. Неканцелируемая операция удерживает owner до своего окончания. Отсоединять её можно только без коротких captures либо после явного безопасного переноса владения.

Основной путь — явные bindings к Effect и JS API. В целевой план также входит importer поддерживаемого подмножества `.d.ts`, который генерирует те же bindings, явно показывает пробелы и допускает локальные исправления контракта. Подключение npm не требует ожидать полного переноса системы типов TS.

Codegen первой версии превращает прямую self tail recursion чистой функции в цикл, сохраняя порядок вычисления аргументов. Обычная нетерминальная и взаимная pure-рекурсия не получают общей гарантии stack safety; для них доступны явный loop/trampoline API. Рекурсивное runtime-выполнение строится через suspend/continuation, без eager рекурсивного создания всей цепочки Effect. Pure loops не приобретают автоматические cancellation checkpoints; неограниченную последовательность runtime-шагов превращать в такой loop запрещено.

## Прямой FFI и цена автоматизации

Основной путь выбран: прямые типизированные bindings к JavaScript, фиксированный ABI данных и ограниченная TS-проекция. Генератор из `.d.ts` дополняет этот путь. Подход с явным типом foreign-функции и связью с JS-экспортом близок к [PureScript FFI](https://book.purescript.org/chapter10.html); [Elm ports](https://guide.elm-lang.org/interop/ports.html) остаются примером отдельной архитектуры обмена сообщениями.

| Подход | Выигрыш | Цена |
| - | - | - |
| Message ports | Явная граница владения состоянием и сообщениями | Каждый небольшой вызов требует протокола; трудно свободно использовать JS API |
| Явные foreign bindings | Предсказуемая связь типов, эффектов и JS-вызова | Контракт пишет и проверяет автор binding |
| Генерация bindings из поддерживаемых `.d.ts` | Меньше ручной работы для типичных библиотек | Не выводит чистоту, runtime-валидацию, отмену и владение данными |
| Импорт всей выразительности TS | Максимальный охват npm | Потребуется сложное согласование двух систем типов; conditional types, overloads и HKT не переносятся автоматически |

Importer `.d.ts` строит явные bindings, использует TypeScript для разрешения деклараций и развивает поддерживаемое подмножество по конкретным пакетам. Неподдержанный экспорт остаётся именованным blocked item с причиной; корректные независимые экспорты пакета доступны. Непроверенные типы не подменяются `any` или чистыми исходными типами. Сгенерированный файл и проверенные overrides разделяются, чтобы обновление пакета не стирало принятые контракты.

### Поддерживаемое подмножество d.ts

| Поверхность первой версии | Результат импорта | Явный контракт |
| - | - | - |
| Примитивы, literals, tuples, arrays, формы объектов, opaque foreign classes | Представимый source type или foreign handle | ABI, mutability/ownership и валидация |
| Optional/nullable и discriminated unions | Только однозначное отображение в выбранный ABI | Различие missing/null/undefined и constructor tags |
| Обычные rank-1 generics | Поддерживаются, если параметризация и представление сохраняются | `.d.ts` не доказывает parametricity |
| Конечные однозначные overloads | Отдельно именованные wrappers | Неразрешимая неоднозначность блокирует export |
| Concrete mapped/conditional/indexed-access types | Только после полного сведения TS к поддерживаемой форме | Зависимые несведённые type operators не поддерживаются |
| Promise и callbacks | Binding shape генерируется; activation требует adapter policy | Errors, owner, retention, cancellation/quiescence, supervisor |
| `any` и TS `unknown` | `JsValue` | Не превращаются в доверенный source Unknown |
| HKT, source classes, regions/captures | Родной `.iface` или конкретный façade | Bare `.d.ts` не является полным source-интерфейсом |

По каждому выбранному export importer выдаёт один из трёх статусов: поддерживается; нужна policy/adapter; не поддерживается. Поддержанные независимые exports доступны, даже если другой export blocked. Простая TS object shape по умолчанию остаётся foreign shape с runtime-доступом до checked snapshot/adapter; она не становится immutable source record из-за `readonly`. Расширение покрытия идёт по версиям importer и не меняет смысл уже проверенного binding.

### Пример импортируемой функции

Гипотетическая библиотека объявляет:

```ts
export declare function lookup(
  id: string
): Promise<string | undefined>;
```

При явно выбранной политике `undefined -> None` и rejection `-> JsError` importer может создать:

```flick
lookup : String ->{; JsError} Option String
foreign async lookup from "package" "lookup"
  arity 1
  catch JsError.from_js
  nullable
```

Преобразование результата задаётся metadata/adaptor binding: `Option` в ABI — нативное значение Effect, поэтому `undefined -> None` записывается строкой `nullable`, а не выводится из типа результата. Политики пакета importer читает из `policy.toml` рядом со сгенерированным модулем (`undefined = "none"`, `catch = "JsError"`, `retention`, `cancel`, `mode = "pure"`, `strict`) и никогда его не пишет. Заблокированный export генерируется как `foreign blocked name "причина"`: его использование — ошибка компиляции, остальные exports пакета доступны. Проверенный вручную binding несёт metadata `reviewed "<pkg>@<ver> sha256:…"` — отпечаток декларации, против которой его проверили; `ffi check` сравнивает его с установленной версией. Promise создаётся при выполнении Effect, а не при построении task. В этой сигнатуре нет доказательства cancellation; сигнал добавляется только для реально поддерживающего его JS API.

Другой экспорт:

```ts
export declare function nextId(): number;
```

консервативно получает `Unit ->{} Number`. Функция может читать часы или менять счётчик. Pure-стрелка требует проверенного контракта; по одному возвращаемому типу её вывести нельзя.

### Диагностика и обновление пакетов

Importer выдаёт отчёт для выбранной поверхности пакета и именованную причину каждого blocked export. `any` и TS `unknown` отображаются в `JsValue`, либо отклоняются строгим режимом пакета; тихого weakening source-типа нет. Overrides хранятся отдельно от генерируемого файла и проверяются после обновления зависимости.

Контракт хранит происхождение декларации и версию зависимости; обновление повторно проверяет shape и совместимость overrides. Overloads можно импортировать как отдельно именованные bindings, если безопасное автоматическое разрешение не определено.

Callback с неизвестным retention не принимает scoped-handle или захватывающую его closure. Нужен договор «только на время вызова», scope-owned registration либо явная unsafe-граница. TS `readonly` не исключает изменения через другой alias; это прямо разобрано в [TypeScript object types](https://www.typescriptlang.org/docs/handbook/2/objects.html).

[ReScript FFI](https://rescript-lang.org/docs/manual/bind-to-js-function/) полезен как пример декларативного JS ABI: методы, `this`, число аргументов и вариативные вызовы. Его подходы можно использовать независимо от выбранной source-системы типов.

Content Mapper связывает исходники и TS-проекцию. Поддержку импортируемого типа, region-checking и корректность runtime adapter обеспечивает frontend; увеличение возможностей Mapper само по себе не переносит семантику TS во Flick.

## Одна сигнатура и явный режим foreign вызова

Выбраны режимы `foreign pure`, `foreign sync`, `foreign async`, `foreign effect`. Полная source-сигнатура стоит над объявлением; `arity`, `this`, `catch`, nullable mapping, retention и cancellation задаются binding metadata. Имена `cancel signal` и расположение сигнала определяют передачу аргумента, но не заменяют контракт quiescence.

```flick
hypot : Number -> Number -> Number
foreign pure hypot from global "Math.hypot"
  arity 2

now_millis : Unit ->{} Number
foreign sync now_millis from global "Date.now"
  arity 0

read_text : Path ->{; FsError} String
foreign async read_text from "./fs.js" "readText"
  arity 1
  catch FsError.from_js
  cancel signal

fetch_user : UserId ->{Http; HttpError} User
foreign effect fetch_user from "./users.js" "fetchUser"
  arity 1
```

`arity` задаёт число обычных позиционных аргументов JS-вызова. `arity 2` адаптирует две стадии каррирования; `arity 0` принимает исходный `()` и вызывает JS без аргументов. Пара `(Number, Number)` сама по себе не разворачивается в два аргумента. `cancel signal` отдельно добавляет сигнал согласно binding, в этом примере последним аргументом.

Источник — `from global "Math.hypot"` для глобального JS-значения или `from "./fs.js" "readText"` для export ESM-модуля. Metadata — строки под объявлением с отступом, по одному элементу на строку: имя и аргументы-атомы. Известные имена: `arity N`, `this`, `catch f`, `cancel signal`, `nullable …`, `retention …`, `optional …`, `variadic …`; неизвестное имя — ошибка `FLICK-S011`. Parser проверяет только форму «имя и аргументы»; допустимость аргументов каждого элемента для данной сигнатуры и режима проверяет FFI-проверка checker.

Удаление физического аргумента `Unit` не удаляет вычисление, которое его произвело: `now_millis (Log.info message)` сначала выполняет логирование в исходном порядке.

JS-адаптер `readText` принимает путь и `AbortSignal`, `FsError.from_js` явно переводит отказ в region-free ошибку. Promise создаётся только при runtime-выполнении. Ниже допустимый lowering для операции, у которой нет переданного scoped-handle; он не служит универсальной реализацией safe async FFI с захваченным ресурсом:

```ts
export const readText = (
  path: string
): Effect.Effect<string, FsError> =>
  Effect.tryPromise({
    try: (signal) => readTextJs(path, signal),
    catch: toFsError
  })
```

`foreign effect` обязан отложить вызов всей JS-фабрики: TS-тип функции с результатом Effect не доказывает чистоту её построения.

```ts
const fetchUser = (id: UserId) =>
  Effect.suspend(() => fetchUserJs(id))
```

Вызов выполняется при запуске в текущем fiber. Throw фабрики становится defect, если binding явно не переводит его в typed error. Отдельный доверенный `foreign pure` может чисто строить `Eff`, но требует более сильного контракта. Эти исходные декларации различаются даже при одинаковом TS-типе. [Effect.suspend](https://effect.website/docs/v4/getting-started/creating-effects).

Throw и rejected Promise переводятся в typed error только по объявленному mapping; иначе остаются defects. Mapping не может поместить scoped capability в error payload. Для scoped arguments обязательны retention и quiescence contract. Ошибка импорта/инициализации внешнего модуля также происходит в runtime и обрабатывается его binding. [Создание Effect](https://effect.website/docs/v4/getting-started/creating-effects).

## Представление значений и callbacks

| Source value | ABI первой версии |
| - | - |
| Unit | JS `undefined`; успешный foreign void нормализуется в `()` |
| Number, BigInt, Bool, String | Соответствующие JS primitives |
| Tuple | Dense JS array фиксированной длины |
| List | Dense JS array с immutable source API |
| Record | Plain JS object с точным набором собственных data fields |
| Пользовательский ADT | Object с `_tag` = `Имя#hash`, где hash — 48 старших бит FNV-1a 64 квалифицированного constructor id; payload в `_0`, `_1` и далее |
| Nominal wrapper | Реальный tagged object по умолчанию; `unboxed type` объявляет представление полем, `untagged type` — различимыми по JS-типу полями |
| Native Effect types | Нативный ABI закреплённой версии; реальные constructors/protocols |
| Unknown input из JS | Opaque JsValue до checked runtime import |
| Nullable/optional | Явный adapter различает null, undefined и отсутствие ключа |
| Methods | Сохраняются this, arity, порядок вычисления и listener identity |
| Callbacks | Четыре явных семейства: pure, Effect, Promise, scoped event |

FFI поддерживает фиксированные uncurried вызовы, `this`, optional arguments и variadic tail через явный List adapter. Currying Flick сохраняется. Constructor id включает номинальную идентичность пакета/модуля/типа/конструктора и фиксируется в `.iface` (поле `id` рядом с `tag`); `_tag` несёт имя конструктора и хеш этого id, что короче полного id в бандле и не сводит идентичность к одному имени. Immutable data по умолчанию копируются на недоверенной входной и выходной границе. Opaque native handles передаются только своим adapter, без попытки скопировать mutable internals. [Bindings ReScript](https://rescript-lang.org/docs/manual/bind-to-js-function/) дают примеры таких соглашений о вызове.

Для JS-callbacks выбраны **четыре явных семейства адаптеров**. Дополнительная sync-модальность и автоматическое превращение callback `void` в запуск Effect не входят в первую версию. Runtime-стрелка с пустыми строками не доказывает синхронность.

| Семейство | Что получает JS | Кто управляет выполнением | Где использовать |
| - | - | - | - |
| Pure callback | Обычное значение результата | Прямой вызов | Comparator, `map`, predicate, синхронное преобразование |
| Effect callback | Ленивое значение `Effect<A, E, R>` | Принимающий Effect API | Нативные Effect combinators; сохраняется текущий fiber и окружение |
| Promise callback | `Promise<A>` | Явно предоставленный runtime и adapter | API, которое действительно ожидает Promise |
| Scoped event callback | Обычно `void`; fiber/очередь принадлежат подписке | Владелец Scope и supervisor | DOM events, WebSocket, event emitter, внешние уведомления |

### Цена выбранного callback ABI

| Подход | Польза | Цена |
| - | - | - |
| Явные четыре семейства — выбрано | Контракт виден в binding; управляемые ошибки, отмена и lifetime | Больше деклараций адаптеров; нельзя передать любую стрелку в любой callback slot |
| Sync-модальность вне первой версии | Можно безопасно допускать ограниченные синхронные действия с немедленным результатом | Одних `r/e` недостаточно: нужны доказательства отсутствия suspension и правила FFI; синхронное действие не становится чистым |
| Автоматический bridge вне первой версии | Короткий исходный код для простых интеграций | `void` не говорит, наблюдает ли JS Promise, ждёт ли его и кто отменяет работу; нужны доверенные annotations и всё те же runtime-контракты |

`Effect.runSync` не является универсальным adapter для `->{}`: при async-границе он бросает `AsyncFiberError`, а начатое вычисление может продолжать работать. Это поведение прямо описано в [Running Effects](https://effect.website/docs/v4/getting-started/running-effects).

### Подписки и обработчики событий

```flick
on_message : Message ->{Log; none} Unit
let on_message message =
  Log.write message.text

start_listening : Unit ->{Scope, Log; none} Unit
let start_listening () =
  Events.subscribe_messages
    (Events.queue 64)
    Events.log_failures
    (fun message -> task { on_message message })
```

Здесь `Events.queue 64` задаёт bounded FIFO policy, `Events.log_failures` — явный supervisor стандартной библиотеки, а callback создаёт холодную задачу. Переполнение отвергает новое событие и сообщает supervisor, не увеличивая очередь бесконечно. Adapter сохраняет environment и listener identity, допускает reentrant вызов во время регистрации только после инициализации owner/очереди, синхронно запрещает late callbacks при закрытии, снимает listener, прерывает и joins задачи и только затем освобождает captures. Snapshot сообщения делается до передачи в fiber. Создание runtime вне приложения возможно лишь через явный boundary adapter.

Late callback после начала закрытия не запускает пользовательскую задачу. Повторная регистрация создаёт отдельного owner/queue; удаляется именно зарегистрированная JS-функция. Binding не превращает события в независимые забытые `runPromise`.

Если JS требует немедленного действия — например, решения о default action события, — асинхронная часть не сможет обещать его вовремя. Возможен binding из двух частей: чистый callback немедленно возвращает решение, adapter выполняет требуемое синхронное действие, затем запускает scoped обработку.

### Higher-rank callback на FFI

`(forall a. a -> a)` экспортируется в поддерживаемом подмножестве как TS-callback `<A>(value: A) => A`. Полиморфизм принадлежит самому callback, а не только внешней функции. HKT и class constraints требуют явного encoding/dictionary ABI либо конкретного фасада; тип `.d.ts` не доказывает parametricity неизвестной реализации.

Для всех семейств binding сохраняет JS arity, `this`, identity обработчика для удаления и согласованное преобразование исключений. Promise-граница отдельно определяет отображение typed errors, defects и interruption; отмена требует реального протокола, например `AbortSignal`, и не возникает из самого типа Promise.

Прямой FFI и валидация данных решают разные задачи. Передача доверенного `HTMLElement` как opaque handle не требует JSON-кодека. Данные HTTP, конфигурация или неизвестный JS-объект требуют проверки до превращения в `User`. Подпись `.d.ts` сама по себе не проверяет значение и не доказывает отсутствие мутации.

Выбрано разделение **`Unknown` и `JsValue`**. `Unknown` — неизвестное source-значение с сохранённым capture provenance: доступная для структурного обхода часть инертна и immutable, доверенные opaque leaves остаются непрозрачными. `JsValue` — произвольный JS-граф с возможными getters, proxies, mutable aliases и callbacks. `Unknown^{s}` не может скрыть capability `s` от scope-checker.

Переход `JsValue -> Unknown` — runtime snapshot/decoder с лимитами размера и глубины. Первая версия принимает поддерживаемый data graph, копирует его, отвергает accessors, функции, неподдержанные prototypes и циклы; shared поддеревья допустимы. Чтение descriptors Proxy может выполнить foreign код, поэтому импорт не объявляется чистым и исключения проходят через явный mapping. После импорта pure Schema не читает mutable opaque internals. Один brand check `Ref` не доказывает тип его содержимого; для generic opaque leaf нужен certified witness. Отдельный `DataValue` в первую версию не вводится.

## Публичные TypeScript интерфейсы

| Исходный экспорт | TS API первой версии |
| - | - |
| Чистая функция | Обычная функция с представимыми TS-типами |
| Runtime-функция | Функция с результатом `Effect<A, E, R>` |
| Представимые constraints | Явный dictionary-record либо специализированный façade |
| Абстрактный HKT | Конкретный façade; непредставимый export диагностируется |
| Opaque nominal type | Branded/tagged оболочка и безопасные constructors |
| Schema/codec | Соответствующее native Effect Schema value и ограниченная TS-проекция |
| Scoped capability/callback | Owner adapter; безусловный bare TS export запрещён |

Каррированный экспорт сохраняет стадии исходной функции. Автоматический uncurried-фасад допускается, когда промежуточные применения лишь создают closures либо доказанно завершаются без наблюдаемого сбоя; одной чистоты префикса недостаточно. Для `f x y` первая стадия `f x` выполняется до вычисления `y`, а JS сначала вычисляет все аргументы обычного вызова. Если первая стадия расходится, даёт defect или выполняет runtime-операцию, преобразование может изменить поведение. В таких случаях сохраняется staged API или нужен явный фасад с отдельно описанным порядком. Внутренний codegen сохраняет последовательность через IR.

Default export runtime-функции возвращает Effect и ничего не запускает при импорте библиотеки. Opt-in Promise façade требует предоставленных сервисов, явного owner и cancellation policy. Основная форма возвращает `Promise<Result.Result<A, Errors<E>>>`: selected expected failure становится Result, defect/interruption отклоняет Promise opaque `RuntimeFailure` с безопасным CauseReport. Отмена задаётся отдельным signal/handle и сохраняет quiescence. Promise не получает фиктивный typed rejection. Внешний façade не может вернуть короткоживущую capability после закрытия своего owner.

Родной `.iface` сохраняет kinds, constraints, стрелки, rows, nominal identities, captures/lifetime и dictionary ABI. `.d.ts` — проверяемая ограниченная проекция. Изменение data layout, marshalling или версии native Effect ABI требует проверки совместимости интерфейса. Непредставимую гарантию нельзя молча удалить при экспорте.

## Оптимизация единственного Effect backend

У Flick один backend, стандартная библиотека ориентирована на Effect. Оптимизация выбирает подходящую генерацию внутри него:

| Исходный код | Генерация |
| - | - |
| Чистая функция | Обычная JS/TS-функция без запуска fiber и Effect-обёртки |
| Чистая работа с данными Effect | Вызовы модулей данных Effect без запуска runtime |
| Runtime-функция | Ленивое вычисление Effect, возвращающее исходный результат при выполнении |
| Чистый участок внутри runtime-тела | Обычные выражения внутри отложенного тела |
| `task` | First-class Effect с отложенным телом |
| Внешний entry point | Предоставление сервисов и один явный запуск Effect |

```ts
import { Effect } from "effect"

const normalizeName = (raw: string): string =>
  raw.trim()

const readName = (
  readRawName: () => string
): Effect.Effect<string> =>
  Effect.sync(() => normalizeName(readRawName()))
```

Здесь `normalizeName` выполняется обычным JS. Весь вызов `readRawName` и нормализация результата остаются внутри ленивого runtime-тела; этот пример иллюстрирует синхронный адаптер, а не отдельный способ исполнения языка.

### Профили оптимизации одного Effect backend

| Профиль | Что разрешено | Цена |
| - | - | - |
| Консервативный lowering | Прямое отображение runtime-шагов, простые локальные преобразования, обычный JS для pure тела | Больше closures и combinators; удобен как эталонный/debug режим |
| Типизированный IR с сохранением границ — выбранный release режим | Inlining, специализация dictionaries и callbacks, безопасное uncurrying, удаление административных обёрток, локальный CSE и fusion при доказанных условиях | Нужен анализ purity, частичности, identity, escape и момента выполнения |
| Агрессивная специализация после первой версии | Дополнительно локальное состояние, статически известные сервисы, loops и representations | Дорогой анализ aliasing/FFI и отдельное обоснование каждого преобразования |

Все профили используют тот же runtime. Чистые функции остаются обычным JS; `->{}` не становится чистой стрелкой. Пустые `r/e` не доказывают отсутствие IO, Ref, suspension, дефекта или наблюдаемого порядка.

| Преобразование | Условие допустимости |
| - | - |
| Специализация class dictionary | Сохраняет выбранный instance, equality/hash и точки вычисления аргументов |
| Uncurrying | Префиксы не расходятся и не бросают исключение до аргумента, чьё вычисление переносится |
| Constant folding | Точная JS Number/BigInt семантика и сохранение момента возможного исключения |
| CSE / удаление вычисления | Доказаны чистота, допустимая частичность и ненаблюдаемость allocation/identity через FFI |
| Fusion pure-участка | Выполнение остаётся внутри той же отложенной области; сохраняется порядок наблюдаемых runtime-действий |
| Слияние runtime-шагов | Сохранены handlers, provide, finalizers, explicit yield и необходимые возможности отмены |

Любой pass также сохраняет lexical owner, captures, исходный Cause при rethrow и quiescence до release. Точное число allocations и автоматических scheduler ticks не является контрактом, но обязательные точки отмены, explicit yield и ровно однократные finalizers удалять нельзя.

Запрещённые без отдельного доказательства примеры: `(a + b) + c -> a + (b + c)`, `x * 0 -> 0`; memoization повторного запуска task; слияние отдельных построений Layer; перенос service lookup через provide; удаление Schema-проверки Unknown на основании TS-имени типа.

Тело `task` не выполняется при построении; два запуска остаются двумя запусками. Eager API Effect нельзя подставлять механически вместо ленивой композиции. Перемещение partial pure вычисления тоже может изменить программу: например, exception при построении вместо exception во время выполнения.

Наблюдаемый контракт включает порядок исходных эффектных вызовов, границы handlers/provide/Scope, регистрацию finalizers, явные yield и смысл interruption. Точное автоматическое расписание fibers и победитель настоящей гонки между готовыми ветками не фиксируются между сборками. [Effect scheduler](https://effect.website/docs/v4/api/effect/Scheduler#MaxOpsBeforeYield) использует счётчик runtime-операций для cooperative yielding, поэтому fusion влияет на моменты переключения. Compiler обязан сохранять возможность кооперативного прогресса и не превращать неограниченную последовательность runtime-шагов в блокирующий бесконечный JS-loop.

Семантика числа и equality — разные основания: даже при Eq, объединяющем signed zero, оптимизация обязана сохранить различимые арифметические результаты. Более сильные режимы возможны только с явным изменением числового контракта; они не входят в выбранную JS-семантику.

### Оптимизации по образцу ReScript

Выбрано направление: адаптировать подходящие оптимизации ReScript к собственному типизированному IR. Проверка типов, эффектов, instances и regions выполняется до преобразований; необходимые доказательства и исходные диапазоны сохраняются в IR.

В просмотренном [pipeline ReScript](https://github.com/rescript-lang/rescript/blob/81d63a8b78aa9083fed02df6a3d4706313f23cc0/compiler/core/lam_compile_main.ml#L291-L326) есть устранение aliases, flattening, повторные simplification/inlining, удаление let-bindings и scalar replacement. Это ориентир для passes и их взаимодействия, а не готовый optimizer для Effect-контрактов Flick.

| Приоритет | Приём | Применение в backend Flick |
| - | - | - |
| Первый набор | Local inlining, beta reduction, alias/constant propagation | Убирают служебные функции и известные применения, сохраняя порядок и момент выполнения |
| Первый набор | Arity analysis и прямой worker-вызов | Полные известные применения обходятся без промежуточных closures; partial application сохраняет currying |
| Первый набор | Pure self-tail recursion в loop | Stack-safe чистые обходы без зависимости от TCO JS-движка |
| Первый набор | Устранение временных records/tuples | Объект заменяется полями, если его representation/identity не наблюдается |
| Первый набор | DCE и подходящие для bundler ES modules | Удаляются недостижимые чистые построения; эффекты и module initialization сохраняются |
| По явному ABI | Unboxed nominal wrappers (`unboxed type`) и различимые JS unions (`untagged type`) | Полезны для FFI; canonical Effect-типы сохраняют своё представление |
| Следующий этап | Ограниченный межмодульный inlining | Versioned summaries с arity, эффектами, captures, ABI и ограничением роста кода |
| Следующий этап | Локальное устранение mutable blocks | Только после escape/alias/identity анализа; не общее правило для Effect Ref |

#### Прямые вызовы с сохранением каррирования

```flick
add : Number -> Number -> Number
let add left right =
  left + right
```

Возможная внутренняя генерация:

```js
function add$2(left, right) {
  return left + right;
}

const add = left => right => add$2(left, right);
```

Известное полное применение вызывает `add$2(x, y)`; частичное сохраняет closure. Worker не обязан становиться публичным TS-экспортом.

У [ReScript uncurried calls стали моделью по умолчанию](https://rescript-lang.org/blog/uncurried-mode/). У нас сохраняется выбранная ML-семантика стадий: если первый вызов выполняет runtime-действие, расходится или бросает исключение до следующего аргумента, механическое объединение вызовов меняет программу. Поэтому переносится arity optimization, а не другая семантика функций.

#### Устранение временных allocations

```flick
sum_pair : Number -> Number -> Number
let sum_pair left right =
  let pair = { left = left; right = right }
  pair.left + pair.right
```

Возможный внутренний worker:

```js
function sumPair$2(left, right) {
  return left + right;
}
```

Точный [SROA pass ReScript](https://github.com/rescript-lang/rescript/blob/81d63a8b78aa9083fed02df6a3d4706313f23cc0/compiler/core/lam_pass_sroa.ml) занимается локальными mutable blocks с проверенными прямыми field reads/writes. Это не разрешение заменить произвольный `Effect.Ref` переменной: у Ref может быть sharing между fibers, внешний alias или наблюдаемая identity. Для начала проще устранять доказанно локальные immutable records/tuples.

#### Вычисление литеральных выражений

Как ReScript, compiler вычисляет на этапе компиляции выражения, составленные только из литералов: `60 * 60 * 24` становится `86400`, `"Hello, " ++ "world"` — одной строкой, `not false` — `true`, `if true then a else b` — значением `a`. Результат обязан бит-в-бит совпадать с выполнением в JS: IEEE 754 для `+ - * / %` на Number, SameValueZero для `==` на Number, порядок UTF-16 code units для `<` на String, усечение к нулю для `/` и `%` на BigInt.

| Выражение | Вычисляется при компиляции |
| - | - |
| Литералы Number, BigInt, String, Bool; скобки | Значение литерала |
| Унарные `-`, `not`; `+ - * / %` на Number | Да |
| `**` на Number | Только при нулевой степени или целом результате, точно представимом в binary64; иначе остаётся runtime-вычисление, потому что `Math.pow` движков может отличаться в последнем бите |
| `+ - * / % **` на BigInt | Пока результат помещается в 128 бит; деление на `0n` и отрицательная степень остаются runtime-исключением `RangeError` (defect) |
| `==`, `!=`, `<`, `<=`, `>`, `>=` | На Number, BigInt и String; на Bool — только `==` и `!=` |
| `&&`, `\|\|`, `++` | Только когда оба операнда константны: свёртка не отбрасывает вычисление с эффектом |
| `if` с константным условием | Значение выбранной ветки, если она константна |

Свёртка применяется к проверенной программе: смешение типов, например `1 + 1n`, отвергает checker, а не optimizer.

Значения литералов. В строке `"…"` действуют escapes `\n \t \r \\ \" \0 \u{…}`. Raw-строки `r"…"`, `r#"…"#` escapes не имеют. В `"""…"""` убираются перевод строки после открывающих кавычек, последняя строка из одних пробелов перед закрывающими и общий отступ непустых строк (строка закрытия тоже учитывается); escapes декодируются после этого, поэтому `\u{20}` — содержимое, а не отступ. Перевод строки внутри литерала всегда `\n`, в том числе в исходнике с `\r\n`. Числовой литерал округляется до binary64 так же, как в JS.

#### Оптимизация классов

Уникальный canonical instance позволяет специализировать dictionary, получить прямой вызов метода и затем применить inlining. Эта часть — наша адаптация к классам типов. При нескольких разных dictionaries одного типа optimizer обязан сохранять выбранный dictionary; одного имени constraint недостаточно.

#### Что сохраняет контракт

[ReScript компилирует tail recursion в loops](https://rescript-lang.org/docs/manual/function/). Для runtime-кода Flick loop дополнительно сохраняет ленивость, границы handlers/Scope и cooperative yielding. Неограниченную runtime-рекурсию нельзя безусловно превращать в синхронный JS-loop.

[ReScript DCE](https://rescript-lang.org/docs/manual/introduction/) использует информацию о чистоте. Во Flick неиспользуемое `task { Log.info message }` можно удалить как чистое описание, если само построение не имеет наблюдаемого действия. Выполненное `Log.info message` нельзя удалить из-за неиспользованного результата `Unit`.

[Unboxed variants ReScript](https://rescript-lang.org/docs/manual/variant/) допустимы при различимых runtime-представлениях. Для opaque newtype Flick erasure не снимает ограничений Eq/Hash: особое поведение может требовать оболочки или dictionary adapter. Native `Option`, `Result`, `Ref`, `Schema` и `Redacted` Effect не меняют ABI ради этой оптимизации.

Представление объявляется явно и одинаково во всех профилях, потому что это ABI, а не решение optimizer:

```flick
unboxed type UserId = UserId String
  deriving Eq, Hash, Show, Schema

untagged type Key = ByNumber Number | ByName String | ByPath (List String)
```

Значение `unboxed type` — поле его единственного конструктора с одним полем. `opaque unboxed type` скрывает конструктор от импортёров так же, как `opaque type`. `untagged type` — поле каждого конструктора; поля различимы по JS-типу (`Number`, `String`, `Bool`, `BigInt`, один list/tuple), и checker отвергает пересечение (`FLICK-T020`). Сопоставление с образцом проверяет `typeof`/`Array.isArray` вместо `_tag`. Derived `Eq`, `Ord`, `Hash`, `Show` работают по представлению; wire-форма `deriving Schema` у `unboxed type` — форма поля, у `untagged type` derived witness нет, его собирают combinators `Schema`. TS-проекция — тип поля или объединение типов полей.

У [ReScript int имеет 32-битную семантику](https://rescript-lang.org/docs/manual/primitive-types/). `Number` во Flick следует JS, поэтому `(a + b) | 0` и замена обычного умножения на `Math.imul` без отдельного доказательства запрещены.

Любой inlining имеет budget по размеру и глубине. Межмодульный pass использует source-интерфейсы с семантическими сведениями, а не пытается восстановить их из `.d.ts` или Content Mapper.

Отдельные модули после оптимизации могут состоять только из обычных JS-функций. Это свойство сгенерированного кода, а не самостоятельная цель другого backend. При появлении runtime-операций используются Effect, его fibers, scopes, Context и Layers.

**Решение FFI/codegen:** выбранный ABI, четыре семейства callbacks, инертный Unknown после runtime import, проверяемое подмножество `.d.ts`, Effect exports и opt-in Promise façade. Debug lowering служит эталоном; release использует typed IR и подходящие passes ReScript. Runtime adapter закрепляет точную версию Effect, проверяет совместимость при обновлении и не подменяет region contracts bare TS-типами.
