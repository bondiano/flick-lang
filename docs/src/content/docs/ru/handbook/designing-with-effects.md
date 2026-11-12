---
title: Проектирование программы с эффектами
description: Отделите логику предметной области от внешних вызовов, выберите реализации сервисов в точке входа и предоставьте тестовые реализации в тестах.
source_hash: e800f34e7820236e
---

Поиск пользователя можно написать одной функцией: вызвать поиск из учебного
JavaScript-модуля, имитирующего внешний источник данных, удалить пробелы в
начале и конце имени и записать результат в журнал. Однако тест такой функции
должен управлять двумя зависимостями: поиском пользователей и записью в журнал.
Сервисы Flick позволяют явно указать эти зависимости в сигнатуре программы
и выбрать их реализации в точке входа.

В этой главе разобран пример
[`examples/first-slice`](https://github.com/bondiano/flick-lang/tree/main/examples/first-slice).
Он загружает двух пользователей, готовит их имена к выводу и обрабатывает
отсутствие пользователя. Мы сохраним преобразование данных чистым, явно
укажем ошибки, подключим внешние реализации и проверим ту же логику на
локальных данных.

Предполагается, что вы уже умеете
[запускать проект Flick](../../start/getting-started/) и читать определения
функций и выражения `match`. Ниже приведены фрагменты модулей; полные версии
для совместного запуска находятся в примере по ссылке.

## Запустите пример

Используйте компилятор и пример из одной ревизии репозитория: эта глава
соответствует ветке `main`. Установите
[зависимости для разработки](https://github.com/bondiano/flick-lang/blob/main/CONTRIBUTING.md#setup)
и выполните команды из корня репозитория:

```sh
cargo build -p flick
./target/debug/flick --project examples/first-slice check
./target/debug/flick --project examples/first-slice run
./target/debug/flick --project examples/first-slice test
```

Программа может вывести:

```text
[log] Loaded Grace Hopper
[log] Loaded Ada Lovelace
Ada Lovelace and Grace Hopper
not found: "u3"
```

Учебный модуль поиска намеренно отвечает на второй запрос быстрее. Две строки
журнала могут появиться в любом порядке, но в итоговом результате имя Ады
предшествует имени Грейс. Команда тестирования запускает три теста: удаление
пробелов в начале и конце имени, две успешные загрузки и отсутствие
пользователя без записи об успехе в журнале.

В примере четыре модуля Flick и небольшой JavaScript-модуль поиска
пользователей:

| Файл | Назначение |
| - | - |
| `src/App/Users.fl` | Типы предметной области, контракты сервисов и логика загрузки |
| `src/App/Live.fl` | Реализации сервисов на основе JavaScript-модуля поиска и консоли |
| `src/App/directory.mjs` | Учебная имитация внешнего источника данных с задержкой ответов |
| `src/App/Main.fl` | Выбор реализаций, обработка ожидаемых ошибок и вывод результатов |
| `test/App/UsersTest.fl` | Тестовые реализации и проверки |

## Отделите чистое преобразование данных

Начните с части программы, которой нужны только данные. В `App.Users`
функция `to_view` удаляет пробелы в начале и конце имени пользователя,
чтобы получить заголовок для вывода:

```flick
opaque type alias UserId = String
  deriving Eq, Show

type alias User = {
  id : UserId;
  name : String;
}

type alias UserView = {
  id : UserId;
  title : String;
}

/// The only way to make a `UserId` outside this module.
user_id : String -> UserId
let user_id raw = raw

to_view : User -> UserView
let to_view user = {
  id = user.id;
  title = String.trim user.name;
}
```

Функция `to_view` выполняет чистое преобразование `User -> UserView`.
Для теста достаточно передать ей значение. Она не загружает пользователя
и не записывает сообщения в журнал. `UserView` — это данные пользователя,
подготовленные для вывода: идентификатор и заголовок, а не визуальный компонент.

`UserId` непрозрачен за пределами `App.Users`: внешний код видит отдельный
тип со скрытым внутренним представлением — в этом примере строкой. Он может
хранить и передавать идентификатор, но не может подставить обычную строку
вместо `UserId` или применять к нему строковые операции. Для создания
идентификатора вызывающий код использует `user_id`. Эта функция не проверяет
содержимое строки: непрозрачность типа сама по себе не гарантирует корректность
идентификатора.

## Почему прямые внешние вызовы усложняют тесты

Без сервисов функция загрузки могла бы вызывать внешние функции напрямую.
Этот вариант приведён для сравнения; в законченном примере объявления
внешних функций через `foreign` находятся в `App.Live`:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# /// The only way to make a `UserId` outside this module.
# user_id : String -> UserId
# let user_id raw = raw
#
# to_view : User -> UserView
# let to_view user = {
#   id = user.id;
#   title = String.trim user.name;
# }
#
type error NotFound = NotFound UserId
  deriving Show

find_name : UserId ->{Async} Option String
foreign find_name from "./directory.mjs" "findName"
  nullable

console_log : String ->{} Unit
foreign console_log from global "console.log"

load_view_direct : UserId ->{Async; NotFound} UserView
let load_view_direct id =
  let user =
    match find_name id with
    | Some name -> { id = id; name = name }
    | None -> from_result (Err (NotFound id))
  let view = to_view user
  console_log "[log] Loaded \{view.title}"
  view
```

Метаданные `nullable` преобразуют значения JavaScript `null` и `undefined`
в `None`. При отсутствии пользователя функция завершается типизированной
ошибкой `NotFound`. Найденный пользователь преобразуется, а подготовленное
имя записывается в журнал.

Функция `from_result` возвращает значение из `Ok` или порождает типизированную
ошибку из `Err`. Здесь она преобразует отсутствие пользователя в ошибку
`NotFound`, указанную в сигнатуре функции.

Функция `load_view_direct` напрямую зависит от `find_name` и `console_log`.
Поэтому тест успешной загрузки должен учитывать задержку модуля поиска и
вывод в консоль. Передача другого идентификатора не заменяет ни одну из
этих зависимостей.

Стрелка `->{Async; NotFound}` означает, что вызов может приостановиться и
завершиться типизированной ошибкой `NotFound`. Она **не** описывает все внешние
возможности: вызовы `foreign` и операции среды выполнения, например вывод в
консоль, пока не все отражаются в строках эффектов Flick. Пустая строка
эффектов не гарантирует изоляции от таких операций. Подробнее — в разделе
[об ограничениях](../../start/why/#ограничения).

## Опишите зависимости через сервисы

Объявите контракты сервисов в `App.Users`, рядом с типами предметной области:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
type error NotFound = NotFound UserId
  deriving Show

service Users =
  get : UserId ->{Async; NotFound} User

service Log =
  info : String ->{} Unit
```

Сервис `Users` объявляет одну операцию `get`. Для вызова `Users.get` нужна
реализация `Users`; вызов может приостановиться (`Async`) или завершиться
типизированной ошибкой `NotFound`. Сервис `Log` объявляет операцию `info`.
Пустая строка эффектов в её объявлении означает, что операция не требует
дополнительных сервисов и не порождает типизированных ошибок. Для вызова
`Log.info` всё равно нужна реализация самого сервиса `Log`.

Определяйте операции по потребностям предметной области. Для загрузки нужен
пользователь по идентификатору, поэтому контракт содержит `get`. Вызывающему
коду не нужно знать, использует ли реализация JavaScript-модуль, пул соединений
или HTTP-клиент.

Теперь программа вызывает эти операции:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
# /// The only way to make a `UserId` outside this module.
# user_id : String -> UserId
# let user_id raw = raw
#
# to_view : User -> UserView
# let to_view user = {
#   id = user.id;
#   title = String.trim user.name;
# }
#
load_view : UserId ->{Async, Users, Log; NotFound} UserView
let load_view id =
  let user = Users.get id
  let view = to_view user

  Log.info "Loaded \{view.title}"

  view
```

Разберём сигнатуру `UserId ->{Async, Users, Log; NotFound} UserView` слева направо:

| Часть | Значение |
| - | - |
| `UserId` | Входное значение |
| `Async` | Вычисление может приостановиться |
| `Users, Log` | Вызывающий код должен предоставить реализации этих сервисов |
| `; NotFound` | Типизированная ошибка, которую вызывающий код должен обработать или передать дальше |
| `UserView` | Успешный результат |

Вызовы в теле выполняются последовательно. Если `Users.get` завершается
ошибкой, выполнение не доходит ни до `to_view`, ни до `Log.info`.
Поэтому для отсутствующего пользователя записи об успехе не будет.

Вызывающая функция тоже должна учитывать используемые операции. В этом
варианте в сигнатуре пропущен `Log`, поэтому компилятор отклоняет программу
с диагностикой `FLICK-E002`:

```flick fail E002
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
load_name : UserId ->{Async, Users; NotFound} String
let load_name id =
  let user = Users.get id
  Log.info "Loaded \{user.name}"
  user.name
```

Добавьте `Log` в строку эффектов или предоставьте его реализацию вокруг вызова.
Сигнатура может разрешать больше эффектов, чем нужно телу, но все требуемые
сервисы и возможные типизированные ошибки должны быть в ней учтены.
Полные правила описаны в [справочнике эффектов](../../reference/effects/).

## Реализуйте сервисы на границе приложения

Модуль `App.Live` содержит объявления внешних функций через `foreign`
и использует их для построения двух слоёв:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
find_name : UserId ->{Async} Option String
foreign find_name from "./directory.mjs" "findName"
  nullable

console_log : String ->{} Unit
foreign console_log from global "console.log"

users_live : Layer none none {Users}
let users_live =
  Layer.succeed Users {
    get = fun id ->
      match find_name id with
      | Some name -> { id = id; name = name }
      | None -> from_result (Err (NotFound id));
  }

log_live : Layer none none {Log}
let log_live = Layer.succeed Log { info = fun line -> console_log "[log] \{line}" }
```

`Layer.succeed Users` принимает запись с реализациями операций сервиса `Users`.
В слое `users_live` реализация `get` преобразует отсутствие результата
поиска (`None`) в ошибку предметной области `NotFound`. В слое `log_live`
реализация `info` записывает сообщения журнала в консоль.

`Layer none none {Users}` описывает **построение слоя**: оно не требует
сервисов, не завершается типизированной ошибкой и предоставляет реализацию
`Users`. Этот тип не означает, что операция `Users.get` чиста или не может
завершиться ошибкой. При вызове она сохраняет контракт с `Async` и `NotFound`.

Теперь `App.Users` не зависит от внешней функции поиска и консоли. Другая
реализация могла бы обращаться к HTTP API с тем же контрактом операции.
Для другой модели ошибок потребовалось бы явно изменить контракт.

## Объедините вычисления, не выбирая реализации

Функция `load_two` в модуле `App.Users` загружает двух пользователей
параллельно:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
# /// The only way to make a `UserId` outside this module.
# user_id : String -> UserId
# let user_id raw = raw
#
# to_view : User -> UserView
# let to_view user = {
#   id = user.id;
#   title = String.trim user.name;
# }
#
# load_view : UserId ->{Async, Users, Log; NotFound} UserView
# let load_view id =
#   let user = Users.get id
#   let view = to_view user
#
#   Log.info "Loaded \{view.title}"
#
#   view
#
load_two : UserId -> UserId ->{Async, Users, Log; NotFound} (UserView, UserView)
let load_two first_id second_id =
  let first = task { load_view first_id }
  let second = task { load_view second_id }

  parallel first second
```

Конструкция `task { ... }` создаёт задачу — отложенное вычисление — без её
запуска. `parallel` запускает обе задачи и возвращает результаты в порядке
аргументов, даже если второе вычисление завершилось раньше. Обе задачи
используют реализации сервисов, предоставленные вокруг вызова; ни одна
не выбирает модуль поиска или способ записи в журнал.

Параллельное выполнение не задаёт общего порядка побочных эффектов разных
задач. Поэтому пример может сначала записать в журнал имя Грейс, но первым
вернуть данные Ады. Тест сортирует строки журнала и отдельно проверяет
порядок результатов.

Если одна из задач завершается ошибкой, `parallel` прерывает вторую и
дожидается её завершения, прежде чем вернуть ошибку. Уже выполненные
эффекты, например запись в журнал, не откатываются. Подробнее — в
[контракте конкурентного выполнения](../../reference/runtime/#конкурентность).

## Подключите реализации в `main`

Модуль `App.Main` импортирует слои из `App.Live`, а операции — из `App.Users`.
Он обрабатывает ожидаемую ошибку там, где можно выбрать сообщение для
пользователя:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
# /// The only way to make a `UserId` outside this module.
# user_id : String -> UserId
# let user_id raw = raw
#
# to_view : User -> UserView
# let to_view user = {
#   id = user.id;
#   title = String.trim user.name;
# }
#
# load_view : UserId ->{Async, Users, Log; NotFound} UserView
# let load_view id =
#   let user = Users.get id
#   let view = to_view user
#
#   Log.info "Loaded \{view.title}"
#
#   view
#
# load_two : UserId -> UserId ->{Async, Users, Log; NotFound} (UserView, UserView)
# let load_two first_id second_id =
#   let first = task { load_view first_id }
#   let second = task { load_view second_id }
#
#   parallel first second
#
# find_name : UserId ->{Async} Option String
# foreign find_name from "./directory.mjs" "findName"
#   nullable
#
# console_log : String ->{} Unit
# foreign console_log from global "console.log"
#
# users_live : Layer none none {Users}
# let users_live =
#   Layer.succeed Users {
#     get = fun id ->
#       match find_name id with
#       | Some name -> { id = id; name = name }
#       | None -> from_result (Err (NotFound id));
#   }
#
# log_live : Layer none none {Log}
# let log_live = Layer.succeed Log { info = fun line -> console_log "[log] \{line}" }
#
describe : String ->{Async, Users, Log} String
let describe raw =
  try (load_view (user_id raw)).title with
  | NotFound id -> "not found: \{show id}"

program : Unit ->{Async, Users, Log} Unit
let program () =
  let both =
    try
      let (first, second) = load_two (user_id "u1") (user_id "u2")
      "\{first.title} and \{second.title}"
    with
    | NotFound id -> "not found: \{show id}"
  Platform.stdout_line both
  Platform.stdout_line (describe "u3")

main : Unit ->{Async; AppError} Unit
let main () = provide Layer.merge users_live log_live { program () }
```

Функция `describe` обрабатывает `NotFound`, поэтому в её сигнатуре этой ошибки
больше нет. Ей по-прежнему нужны реализации `Users` и `Log`, а выполнение
может приостановиться. Аналогичная обработка в `program` формирует сообщение,
если одна из параллельных загрузок не находит пользователя. Так решение
о сообщении остаётся в точке входа, а код предметной области сохраняет
типизированную ошибку.

`Layer.merge users_live log_live` объединяет два слоя.
Блок `provide ... { program () }` предоставляет реализации обоих сервисов,
поэтому `main` больше не требует `Users` и `Log`. `Async` остаётся:
предоставление сервисов не устраняет возможность приостановки. Сигнатура
точки входа разрешает `AppError`, но это не означает, что тело обязано
завершиться такой ошибкой.

В точке входа приложение выбирает внешние реализации. Здесь же находятся
два вызова `Platform.stdout_line`, которые выводят результаты работы
программы.

## Подставьте тестовые реализации сервисов

Тестовый модуль импортирует `App.Users` и предоставляет тестовые реализации
сервисов. Пользователи берутся из фиксированного списка, а сообщения журнала
сохраняются в `Ref`:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
# /// The only way to make a `UserId` outside this module.
# user_id : String -> UserId
# let user_id raw = raw
#
# to_view : User -> UserView
# let to_view user = {
#   id = user.id;
#   title = String.trim user.name;
# }
#
ada : User
let ada = { id = user_id "u1"; name = "  Ada  " }

grace : User
let grace = { id = user_id "u2"; name = "Grace" }

users_test : Layer none none {Users}
let users_test =
  Layer.succeed Users {
    get = fun id ->
      match List.find (fun user -> eq user.id id) [ada, grace] with
      | Some user -> user
      | None -> from_result (Err (NotFound id));
  }

log_test : Ref (List String) -> Layer none none {Log}
let log_test lines =
  Layer.succeed Log { info = fun line -> Ref.update lines (fun xs -> List.append xs [line]) }
```

Слой `users_test` предоставляет реализацию того же контракта без ожидания
внешнего источника данных. Реализация может требовать меньше эффектов, чем
разрешает контракт. Для каждого теста с эффектами создаётся отдельный буфер
журнала, поэтому тесты не используют общие записанные строки.

Модуль экспортирует три теста:

```flick
# opaque type alias UserId = String
#   deriving Eq, Show
#
# type alias User = {
#   id : UserId;
#   name : String;
# }
#
# type alias UserView = {
#   id : UserId;
#   title : String;
# }
#
# type error NotFound = NotFound UserId
#   deriving Show
#
# service Users =
#   get : UserId ->{Async; NotFound} User
#
# service Log =
#   info : String ->{} Unit
#
# /// The only way to make a `UserId` outside this module.
# user_id : String -> UserId
# let user_id raw = raw
#
# to_view : User -> UserView
# let to_view user = {
#   id = user.id;
#   title = String.trim user.name;
# }
#
# load_view : UserId ->{Async, Users, Log; NotFound} UserView
# let load_view id =
#   let user = Users.get id
#   let view = to_view user
#
#   Log.info "Loaded \{view.title}"
#
#   view
#
# load_two : UserId -> UserId ->{Async, Users, Log; NotFound} (UserView, UserView)
# let load_two first_id second_id =
#   let first = task { load_view first_id }
#   let second = task { load_view second_id }
#
#   parallel first second
#
# ada : User
# let ada = { id = user_id "u1"; name = "  Ada  " }
#
# grace : User
# let grace = { id = user_id "u2"; name = "Grace" }
#
# users_test : Layer none none {Users}
# let users_test =
#   Layer.succeed Users {
#     get = fun id ->
#       match List.find (fun user -> eq user.id id) [ada, grace] with
#       | Some user -> user
#       | None -> from_result (Err (NotFound id));
#   }
#
# log_test : Ref (List String) -> Layer none none {Log}
# let log_test lines =
#   Layer.succeed Log { info = fun line -> Ref.update lines (fun xs -> List.append xs [line]) }
#
tests : List Test.Test
let tests = [
  Test.test "to_view trims the name" (fun () -> Test.equal "Ada" (to_view ada).title),
  Test.effect_test
    "load_two loads both and logs each"
    (task {
      let lines = Ref.make []
      let (first, second) = provide Layer.merge users_test (log_test lines) {
        load_two (user_id "u1") (user_id "u2")
      }
      Test.expect (Test.equal ["Ada", "Grace"] [first.title, second.title])
      Test.expect (Test.equal ["Loaded Ada", "Loaded Grace"] (List.sort (Ref.get lines)))
    }),
  Test.effect_test
    "load_view fails with NotFound"
    (task {
      let lines = Ref.make []
      let outcome =
        try
          let view = provide Layer.merge users_test (log_test lines) {
            load_view (user_id "nobody")
          }
          view.title
        with
        | NotFound id -> "missing \{show id}"
      Test.expect (Test.equal "missing \"nobody\"" outcome)
      Test.expect (Test.equal [] (Ref.get lines))
    }),
]
```

Первый тест напрямую проверяет чистое преобразование данных. Второй вызывает
саму функцию `load_two` с тестовыми реализациями сервисов и проверяет
полученные данные и журнал. Третий проверяет обработку ошибки и отсутствие
записи об успехе. Ни одному из тестов не нужно подменять глобальную внешнюю
функцию или перехватывать вывод процесса.

При проверке журнала параллельных вычислений сравниваются отсортированные
строки, а порядок результатов проверяется отдельно. Проверка случайного
порядка завершения сделала бы тест зависимым от планировщика, а не от
контракта загрузки.

## Примените подход в своей программе

При добавлении новой возможности начните со значений, которые она преобразует.
По возможности сохраняйте это преобразование чистым. Для каждой внешней
зависимости объявите нужные операции сервиса. Укажите в сигнатурах требуемые
сервисы, возможность приостановки и ожидаемые типизированные ошибки.
Сохраняйте эти требования до того уровня вызывающего кода, где можно
предоставить реализации или выбрать способ обработки ошибки.

Внешние слои выбирайте на границе приложения, а в тестах предоставляйте
тестовые реализации. Если слой получает ресурс, например соединение,
ограничьте время его жизни областью владения ресурсами. Здесь достаточно
`Layer.succeed`, потому что эти слои не получают ресурсов.
Подробности — в разделах [Слои и provide](../../reference/effects/#слои-и-provide),
[Регионы и захваты](../../reference/regions/) и
[Внешние функции](../../reference/ffi/).
