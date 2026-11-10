---
title: Как проектировать программу с эффектами
description: Отделите логику предметной области от внешних вызовов, подключите сервисы в точке входа и замените их в тестах.
source_hash: 0c4a9d08af5ded51
---

Поиск пользователя легко уместить в одну функцию: вызвать справочник на
JavaScript, убрать пробелы из имени, записать результат в лог. Но для теста
нужно управлять и справочником, и логом. Сервисы Flick позволяют назвать
эти зависимости в сигнатуре программы, а их реализации выбрать в точке
входа.

В этой главе разобран
[`examples/first-slice`](https://github.com/bondiano/flick-lang/tree/main/examples/first-slice).
Он загружает двух пользователей, подготавливает их имена для вывода и
обрабатывает отсутствие пользователя. Мы сохраним преобразование данных
чистым, явно опишем ошибки, подключим внешние реализации и проверим ту же
логику на локальных данных.

Предполагается, что вы уже умеете
[запускать проект Flick](../../start/getting-started/) и читать функцию и
`match`. Ниже приведены фрагменты; полные модули для совместного запуска
лежат в примере по ссылке.

## Запустите пример

Возьмите компилятор и пример из одной ревизии репозитория: эта глава
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

Справочник намеренно отвечает на второй запрос быстрее. Две строки лога
могут идти в любом порядке, но в результате Ада остаётся перед Грейс.
Команда тестирования запускает три теста: удаление пробелов из имени, две
успешные загрузки и отсутствие пользователя без записи об успехе в логе.

В примере четыре модуля Flick и небольшой справочник на JavaScript:

| Файл | Ответственность |
| - | - |
| `src/App/Users.fl` | Типы предметной области, контракты сервисов и логика загрузки |
| `src/App/Live.fl` | Реализации поверх справочника на JavaScript и консоли |
| `src/App/directory.mjs` | Имитация внешнего справочника, который отвечает с задержкой |
| `src/App/Main.fl` | Выбор реализаций, обработка ожидаемых ошибок и вывод результатов |
| `test/App/UsersTest.fl` | Локальные реализации и проверки |

## Оставьте преобразование данных чистым

Начните с части, которой нужны только данные. В `App.Users` из имени
пользователя убираются крайние пробелы, чтобы получить заголовок для вывода:

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
передать значение. Функция не выбирает, откуда взять пользователя и куда
записать лог.

`UserId` непрозрачен за пределами `App.Users`: чтобы создать его, вызывающий
код использует `user_id`. Здесь эта функция оборачивает строку без
валидации; сам по себе непрозрачный тип не делает идентификатор корректным.

## Посмотрите, что связывают прямые внешние вызовы

До введения сервисов загрузка могла бы напрямую вызывать внешние функции.
Это исходный вариант для сравнения; в готовом примере эти привязки находятся
в `App.Live`:

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
пользователь преобразуется, а результат записывается в лог.

`from_result` возвращает значение из `Ok` или порождает типизированную ошибку
из `Err`. Здесь она превращает ветку отсутствующего пользователя в ошибку,
названную в сигнатуре функции.

Эта функция закрепляет оба решения: какой справочник вызвать и куда писать
лог. Тест успешной загрузки должен учитывать задержку этого справочника и
работу с консолью. Передача другого идентификатора не заменяет ни одну из
зависимостей.

Стрелка `->{Async; NotFound}` говорит, что вызов может приостановиться и
завершиться ошибкой `NotFound`. Она **не** описывает все внешние возможности:
вызовы `foreign` и операции хоста, например вывод в консоль, пока не все
обозначаются в строках эффектов Flick. Пустая строка сервисов не изолирует
программу от внешнего мира. Подробнее об этом — в разделе
[об ограничениях](../../start/why/#ограничения).

## Назовите операции, которые нужны программе

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

`Users` обещает одну операцию. Для вызова `Users.get` нужна реализация
`Users`; вызов может приостановиться (`Async`) или завершиться ошибкой
`NotFound`. `Log` обещает операцию `info`. Пустая строка в её объявлении
означает, что операция не требует дополнительных сервисов и не возвращает
типизированных ошибок; для её вызова всё равно нужен сам `Log`.

Описывайте операции через потребности предметной области. Загрузке нужен
пользователь по идентификатору, поэтому контракт содержит `get`. Ему не
нужно открывать каждому вызывающему модуль JavaScript, пул соединений или
HTTP-клиент.

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

Тело читается как обычный последовательный код. Если `Users.get`
завершается ошибкой, до `to_view` и `Log.info` выполнение не доходит.
Для отсутствующего пользователя записи об успехе не будет.

Обёртка тоже должна учитывать используемые операции. В этом варианте в
сигнатуре пропущен `Log`, и компилятор сообщает `FLICK-E002`:

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

Добавьте `Log` в строку или предоставьте его реализацию вокруг вызова.
Сигнатура может разрешать больше, чем нужно телу, но тело не может незаметно
добавить сервис или типизированную ошибку. Полные правила приведены в
[справочнике эффектов](../../reference/effects/).

## Поместите внешние реализации на границу приложения

`App.Live` содержит внешние привязки и собирает из них два слоя:

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

`Layer.succeed Users` принимает запись с операциями `Users`.
`users_live` держит в одном месте преобразование внешнего nullable-результата
в ошибку предметной области `NotFound`. `log_live` решает, как лог приложения
попадает в консоль.

`Layer none none {Users}` описывает **построение слоя**: ему не нужны
сервисы, у него нет типизированной ошибки построения, а предоставляет он
`Users`. Это не означает, что `Users.get` чиста или не может завершиться
ошибкой. При вызове у операции сохраняется контракт с `Async` и `NotFound`.

Теперь `App.Users` не зависит от привязки к справочнику и консоли.
Другая реализация могла бы обращаться к HTTP endpoint с тем же контрактом
операции; другая модель ошибок потребовала бы явного изменения контракта.

## Собирайте вычисления без выбора реализаций

Пример также загружает двух пользователей параллельно, по-прежнему внутри
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

`task { ... }` создаёт вычисление для запуска. `parallel` запускает оба
вычисления и возвращает результаты в порядке аргументов, даже если второе
завершилось раньше. Ни одно из них не выбирает справочник или лог: оба
используют сервисы, предоставленные вокруг вызова.

Параллельное выполнение не упорядочивает побочные эффекты. Поэтому пример
может сначала записать в лог Грейс, но первым вернуть представление Ады.
Тест сортирует записанные строки лога и отдельно проверяет порядок
результатов.

Если одно вычисление завершается ошибкой, `parallel` прерывает другое и
дожидается его завершения, прежде чем вернуть ошибку. Уже выполненные
эффекты, например запись в лог, не откатываются. Подробнее — в
[контракте конкурентного выполнения](../../reference/runtime/#конкурентность).

## Соберите приложение в `main`

`App.Main` импортирует слои из `App.Live`, а операции — из `App.Users`.
Здесь ожидаемая ошибка обрабатывается там, где можно выбрать сообщение для
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
приостановиться. Такое же решение в `program` даёт сообщение, если одна
из параллельных загрузок не находит пользователя. Обработка ожидаемой
ошибки здесь позволяет коду предметной области сохранить её типизированной.

`Layer.merge users_live log_live` предоставляет оба сервиса.
`provide ... { program () }` делает их доступными в теле блока, поэтому
`main` больше не требует `Users` и `Log`. `Async` остаётся: предоставление
сервисов не убирает возможность приостановки. Сигнатура точки входа
разрешает `AppError`; этому телу не требуется её порождать.

Здесь выбираются внешние реализации для этого приложения. Два вызова
`Platform.stdout_line` тоже находятся здесь: они решают, как представить
результаты работы программы.

## Проверяйте логику с другими сервисами

Тестовый модуль импортирует `App.Users` и предоставляет локальные
реализации. Пользователи берутся из фиксированного списка, а лог
записывается в `Ref`:

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

`users_test` реализует тот же контракт без ожидания внешнего справочника.
Реализация может требовать меньше эффектов, чем разрешает контракт.
`log_test` получает новый буфер лога для каждого эффектного теста, поэтому
тесты не делят между собой записанные строки.

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
настоящую `load_two` с тестовыми сервисами и проверяет и полученные
представления, и лог. Третий проверяет путь с ошибкой и отсутствие записи
об успехе. Ни одному из них не нужно подменять глобальную внешнюю функцию
или перехватывать вывод процесса.

Обратите внимание: проверка лога параллельных вычислений сравнивает
отсортированные строки. Проверка случайного порядка завершения сделала бы
тест зависимым от планировщика, а не от контракта загрузки.

## Примените этот подход к своей программе

Добавляя возможность, начните с преобразуемых значений и по возможности
сохраните преобразование чистым. Для каждой внешней зависимости назовите
нужные операции в сервисе. Пусть сигнатура передаёт дальше требования к
сервисам, возможность приостановки и ожидаемые ошибки, пока вызывающий код
не сможет предоставить реализацию или принять решение об обработке ошибки.

Выбирайте внешние слои на границе приложения, а в тестах предоставляйте
локальные. Если слой получает ресурс, например соединение, задайте ему
область жизни. Здесь достаточно `Layer.succeed`, потому что эти слои не
получают ресурсов. Подробности — в разделах
[Слои и provide](../../reference/effects/#слои-и-provide),
[Регионы и захваты](../../reference/regions/) и
[Внешние функции](../../reference/ffi/).
