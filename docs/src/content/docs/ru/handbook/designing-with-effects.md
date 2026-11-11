---
title: Как проектировать программу с эффектами
description: Отделите логику предметной области от внешних вызовов, соберите реализации сервисов в точке входа и подставьте тестовые реализации при тестировании.
source_hash: 520f701027fd0d91
---

Поиск пользователя легко написать одной функцией: вызвать функцию поиска из
учебного JavaScript-модуля, имитирующего внешний источник данных, удалить
пробелы в начале и конце имени и записать результат в лог. Но чтобы
протестировать такую функцию, придётся подменять и поиск пользователей, и
запись в лог. Сервисы Flick позволяют назвать эти зависимости в сигнатуре
программы, а их реализации выбрать в точке входа.

В этой главе разобран
[`examples/first-slice`](https://github.com/bondiano/flick-lang/tree/main/examples/first-slice).
Он загружает двух пользователей, готовит их имена к выводу и обрабатывает
отсутствие пользователя. Мы сохраним преобразование данных чистым, явно
опишем ошибки, подключим внешние реализации и проверим ту же логику на
локальных данных.

Предполагается, что вы уже умеете
[запускать проект Flick](../../start/getting-started/) и читать функцию и
`match`. Ниже приведены фрагменты; полные модули для совместного запуска
лежат в примере по ссылке.

## Запустите пример

Компилятор и пример берите из одной ревизии репозитория: эта глава
соответствует `main`. Установите
[зависимости для разработки](https://github.com/bondiano/flick-lang/blob/main/CONTRIBUTING.md#setup)
и выполните из корня репозитория:

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
лога могут появиться в любом порядке, но в результате Ада остаётся перед
Грейс. Команда тестирования запускает три теста: удаление пробелов в начале и
конце имени, две успешные загрузки и отсутствие пользователя без записи об
успехе в логе.

В примере четыре модуля Flick и небольшой JavaScript-модуль поиска
пользователей:

| Файл | Ответственность |
| - | - |
| `src/App/Users.fl` | Типы предметной области, контракты сервисов и логика загрузки |
| `src/App/Live.fl` | Реализации поверх JavaScript-модуля поиска и консоли |
| `src/App/directory.mjs` | Учебная имитация внешнего источника данных с задержкой ответов |
| `src/App/Main.fl` | Выбор реализаций, обработка ожидаемых ошибок и вывод результатов |
| `test/App/UsersTest.fl` | Тестовые реализации и проверки |

## Отделите чистое преобразование данных

Начните с части, которой нужны только данные. В `App.Users` из имени
пользователя удаляются пробелы в начале и конце, чтобы получить заголовок для
вывода:

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

У `to_view` обычная стрелка: `User -> UserView`. Для теста достаточно
передать значение. Функция сама не решает, откуда взять пользователя и куда
записать лог. `UserView` — это данные пользователя, подготовленные для вывода
(идентификатор и готовый заголовок), а не визуальный компонент.

`UserId` непрозрачен за пределами `App.Users`: внешний код видит отдельный
тип со скрытым внутренним представлением — в этом примере строкой. Он может
хранить и передавать идентификатор, но не может подставить обычную строку
вместо `UserId` или применять к нему строковые операции. Чтобы создать
идентификатор, вызывающий код использует `user_id`. Здесь функция просто оборачивает строку без
валидации; сам по себе непрозрачный тип не делает идентификатор корректным.

## Почему прямые внешние вызовы усложняют тесты

До появления сервисов загрузка могла бы вызывать внешние функции напрямую.
Этот вариант приведён для сравнения; в законченном примере объявления внешних
функций через `foreign` находятся в `App.Live`:

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
  console_log ("[log] Loaded " ++ view.title)
  view
```

`nullable` превращает JavaScript `null` или `undefined` в `None`. Отсутствие
пользователя становится типизированной ошибкой `NotFound`; найденный
пользователь преобразуется, и результат записывается в лог.

`from_result` возвращает значение из `Ok` или порождает типизированную ошибку
из `Err`. Здесь она превращает ветку отсутствующего пользователя в ошибку,
названную в сигнатуре функции.

Функция закрепляет оба решения: какую функцию поиска вызвать и куда писать
лог. Тест успешной загрузки должен учитывать задержку модуля поиска и вывод в
консоль. Передача другого идентификатора не заменяет ни одну из зависимостей.

Стрелка `->{Async; NotFound}` означает, что вызов может приостановиться и
завершиться ошибкой `NotFound`. Она **не** описывает все внешние возможности:
вызовы `foreign` и операции среды выполнения — например, вывод в консоль —
пока не все отражаются в строках эффектов Flick. Пустая строка сервисов не
изолирует программу от внешнего мира. Подробнее — в разделе
[об ограничениях](../../start/why/#ограничения).

## Опишите зависимости через сервисы

Поместите контракты в `App.Users`, рядом с типами предметной области:

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
ошибкой `NotFound`. Сервис `Log` объявляет операцию `info`. Пустая строка
эффектов в её объявлении означает, что операция не требует дополнительных
сервисов и не порождает типизированных ошибок; для её вызова всё равно нужен
сам `Log`.

Описывайте операции исходя из потребностей предметной области. Загрузке нужен
пользователь по идентификатору, поэтому контракт содержит `get`. Вызывающему
коду при этом не нужно знать о JavaScript-модуле, пуле соединений или
HTTP-клиенте.

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

  Log.info ("Loaded " ++ view.title)

  view
```

Прочитаем `UserId ->{Async, Users, Log; NotFound} UserView` слева направо:

| Часть | Значение |
| - | - |
| `UserId` | Входное значение |
| `Async` | Вычисление может приостановиться |
| `Users, Log` | Вызывающий код должен предоставить эти сервисы |
| `; NotFound` | Типизированная ошибка, которую нужно обработать или передать дальше |
| `UserView` | Успешный результат |

Тело читается как обычный последовательный код. Если `Users.get` завершается
ошибкой, до `to_view` и `Log.info` выполнение не доходит. Для отсутствующего
пользователя записи об успехе не будет.

Вызывающая функция тоже должна учитывать операции, которые использует. В этом
варианте в сигнатуре пропущен `Log`, и компилятор отклоняет программу с
`FLICK-E002`:

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
  Log.info ("Loaded " ++ user.name)
  user.name
```

Добавьте `Log` в строку эффектов или предоставьте реализацию `Log` вокруг
вызова. Сигнатура может разрешать больше, чем нужно телу, но тело не может
незаметно добавить сервис или типизированную ошибку. Полные правила описаны в
[справочнике эффектов](../../reference/effects/).

## Реализуйте сервисы на границе приложения

`App.Live` содержит объявления внешних функций через `foreign` и собирает из
них два слоя:

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
let log_live = Layer.succeed Log { info = fun line -> console_log ("[log] " ++ line) }
```

`Layer.succeed Users` принимает запись с операциями сервиса `Users`.
`users_live` держит в одном месте преобразование отсутствия результата
поиска — значения `None` — в ошибку предметной области `NotFound`. `log_live`
решает, как лог приложения попадает в консоль.

`Layer none none {Users}` описывает **построение слоя**: ему не нужны
сервисы, у него нет типизированной ошибки построения, а предоставляет он
`Users`. Это не значит, что `Users.get` чиста или не может завершиться
ошибкой: при вызове операция сохраняет контракт с `Async` и `NotFound`.

Теперь `App.Users` не зависит от внешних функций и консоли. Другая реализация
могла бы обращаться к HTTP API с тем же контрактом операции; другая модель
ошибок потребовала бы явного изменения контракта.

## Объедините вычисления, не выбирая реализации

Пример также загружает двух пользователей параллельно — по-прежнему внутри
`App.Users`:

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
#   Log.info ("Loaded " ++ view.title)
#
#   view
#
load_two : UserId -> UserId ->{Async, Users, Log; NotFound} (UserView, UserView)
let load_two first_id second_id =
  let first = task { load_view first_id }
  let second = task { load_view second_id }

  parallel first second
```

`task { ... }` создаёт вычисление для последующего запуска. `parallel`
запускает оба вычисления и возвращает результаты в порядке аргументов, даже
если второе завершилось раньше. Ни одно из них не выбирает модуль поиска или
лог: оба используют сервисы, предоставленные вокруг вызова.

Параллельное выполнение не упорядочивает побочные эффекты. Поэтому пример
может сначала записать в лог Грейс, но первым вернуть данные Ады. Тест
сортирует записанные строки лога и отдельно проверяет порядок результатов.

Если одно из вычислений завершается ошибкой, `parallel` прерывает второе и
дожидается его завершения, прежде чем вернуть ошибку. Уже выполненные
эффекты, например запись в лог, не откатываются. Подробнее — в
[контракте конкурентного выполнения](../../reference/runtime/#конкурентность).

## Подключите реализации в `main`

`App.Main` импортирует слои из `App.Live`, а операции — из `App.Users`.
Ожидаемая ошибка обрабатывается там, где можно выбрать сообщение для
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
#   Log.info ("Loaded " ++ view.title)
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
# let log_live = Layer.succeed Log { info = fun line -> console_log ("[log] " ++ line) }
#
describe : String ->{Async, Users, Log} String
let describe raw =
  try (load_view (user_id raw)).title with
  | NotFound id -> "not found: " ++ show id

program : Unit ->{Async, Users, Log} Unit
let program () =
  let both =
    try
      let (first, second) = load_two (user_id "u1") (user_id "u2")
      first.title ++ " and " ++ second.title
    with
    | NotFound id -> "not found: " ++ show id
  Platform.stdout_line both
  Platform.stdout_line (describe "u3")

main : Unit ->{Async; AppError} Unit
let main () = provide Layer.merge users_live log_live { program () }
```

`describe` перехватывает `NotFound`, поэтому в её сигнатуре этой ошибки
больше нет. Ей по-прежнему нужны `Users` и `Log`, а выполнение может
приостановиться. Такое же решение в `program` даёт понятное сообщение, если
одна из параллельных загрузок не находит пользователя. Обработка ожидаемой
ошибки здесь позволяет коду предметной области оставить её типизированной.

`Layer.merge users_live log_live` предоставляет оба сервиса.
`provide ... { program () }` делает их доступными внутри блока, поэтому
`main` больше не требует `Users` и `Log`. `Async` остаётся: предоставление
сервисов не убирает возможность приостановки. Сигнатура точки входа разрешает
`AppError`; этому телу не требуется её порождать.

Именно здесь приложение выбирает внешние реализации. Здесь же находятся два
вызова `Platform.stdout_line`: они решают, как показать результаты работы
программы.

## Подставьте тестовые реализации сервисов

Тестовый модуль импортирует `App.Users` и предоставляет тестовые реализации.
Пользователи берутся из фиксированного списка, а лог записывается в `Ref`:

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

`users_test` реализует тот же контракт без ожидания внешнего источника
данных. Реализация может требовать меньше эффектов, чем разрешает контракт.
Для каждого теста с эффектами создаётся отдельный буфер лога, поэтому тесты
не делят между собой записанные строки.

Затем пример экспортирует три теста:

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
#   Log.info ("Loaded " ++ view.title)
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
        | NotFound id -> "missing " ++ show id
      Test.expect (Test.equal "missing \"nobody\"" outcome)
      Test.expect (Test.equal [] (Ref.get lines))
    }),
]
```

Первый тест напрямую проверяет чистое преобразование. Второй запускает
настоящую `load_two` с тестовыми сервисами и проверяет и полученные данные, и
лог. Третий проверяет путь с ошибкой и отсутствие записи об успехе. Ни одному
из них не нужно подменять глобальную внешнюю функцию или перехватывать вывод
процесса.

Обратите внимание: проверка лога параллельных вычислений сравнивает
отсортированные строки. Сравнение случайного порядка завершения сделало бы
тест зависимым от планировщика, а не от контракта загрузки.

## Примените подход в своей программе

Добавляя возможность, начните со значений, которые она преобразует, и по
возможности сохраняйте преобразование чистым. Для каждой внешней зависимости
назовите нужные операции в сервисе. Пусть сигнатура называет требуемые
сервисы, возможность приостановки и ожидаемые ошибки, пока вызывающий код не
сможет предоставить реализацию или решить, как обработать ошибку.

Внешние слои выбирайте на границе приложения, а в тестах предоставляйте
тестовые. Если слой получает ресурс, например соединение, задайте этому
ресурсу время жизни. Здесь достаточно `Layer.succeed`, потому что эти слои не
выделяют ресурсов. Подробности — в разделах
[Слои и provide](../../reference/effects/#слои-и-provide),
[Регионы и захваты](../../reference/regions/) и
[Внешние функции](../../reference/ffi/).
