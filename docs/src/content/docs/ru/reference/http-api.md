---
title: "Типизированные HTTP API"
---

`HttpApi` позволяет описать HTTP API значениями программы: эндпоинты
с кодеками параметров, заголовков и тела, группы эндпоинтов и API
из этих групп (см. [Граница языкового ядра](../../spec/05-errors-and-concurrency/#граница-языкового-ядра)).
Тип API задаёт запись обработчиков: для каждого эндпоинта нужен ровно
один обработчик. Клиент использует то же описание эндпоинта.
Для экспорта в OpenAPI 3.1 используется строгий экспортёр JSON Schema.

```flick
type alias User = { id : String; name : String }
  deriving Schema

type alias UserPath = { id : String }
  deriving Schema

type error NotFound = NotFound { id : String }
  deriving Schema

get_user : HttpApi.Endpoint "get_user" (HttpApi.Request UserPath Unit Unit Unit ->{r; NotFound} User)
let get_user =
  HttpApi.endpoint HttpApi.Get "/users/:id" (Record.label : Record.Label "get_user")
    |> HttpApi.with_path (Codec.json (schema UserPath))
    |> HttpApi.with_success 200 (Codec.json (schema User))
    |> HttpApi.with_errors (HttpApi.errors |> HttpApi.error_case 404 (Codec.json (schema NotFound)))

api : HttpApi.Api r { users : { get_user : HttpApi.Request UserPath Unit Unit Unit ->{r; NotFound} User } }
let api =
  HttpApi.api "Users"
    |> HttpApi.add_group (HttpApi.group (Record.label : Record.Label "users") |> HttpApi.add get_user)
```

<div id="конечные-точки"></div>

## Эндпоинты

`HttpApi.endpoint method path label` создаёт эндпоинт с начальными
настройками: обработчик принимает `Request Unit Unit Unit Unit`
(`{ path; query; headers; payload }`), ответ имеет статус 204.
Сигнатура эндпоинта задаёт тип обработчика. Его строка сервисов `r`
остаётся открытой, а строка ошибок `e` перечисляет типизированные ошибки,
которыми он может завершиться. Группа и API несут сервисы всех своих
обработчиков, `HttpApi.Group n r hs` и `HttpApi.Api r gs`, и `listen` их
требует: обработчик, которому нужен сервис без слоя в месте вызова, не
компилируется.

| Комбинатор | Назначение |
| - | - |
| `with_path codec` | декодирует текстовые параметры пути (`/users/:id`) кодеком записи |
| `with_query codec` | задаёт кодек строки запроса |
| `with_headers codec` | задаёт кодек заголовков по именам в нижнем регистре; заголовки, не названные кодеком, игнорируются |
| `with_payload codec` | задаёт кодек тела JSON |
| `with_text_payload`, `with_form_payload codec` | задают текстовое тело или форму в URL-кодировке |
| `with_success status codec`, `with_text_success status` | задают статус и тело успешного ответа |
| `with_errors (errors \|> error_case status codec \|> …)` | задаёт типизированные ошибки обработчика и способ отправки каждой |
| `with_security scheme` | задаёт `bearer name`, `basic name` или `api_key name { key; location }`: без учётных данных возвращается 401 |
| `with_middleware hook` | добавляет эффектную функцию, которая получает декодированный запрос перед обработчиком |
| `with_auth scheme Service check`, `with_optional_auth …` | даёт обработчику сервис, который `check` строит по каждому запросу (ниже) |
| `describe`, `deprecate`, `hide` | добавляют краткое описание, пометку об устаревании или исключают эндпоинт из документа |

Эндпоинт создаётся без типизированных ошибок: обработчик может завершиться
только ошибкой, для которой `with_errors` задал вариант. Обработчик
с ошибкой без варианта, например новой `Conflict`, — ошибка типов ещё
до запуска. `HttpApi.HttpError { status; message }` отправляется
с указанным в ней статусом там, где её объявляет `http_error status`.
Обработчик потоковой формы может завершиться и ошибкой потока
`MultipartError`; она отправляется с указанным в ней статусом.

Если вариантов несколько, сервер отвечает на ошибку первым вариантом, кодек
которого её принимает, поэтому каждый кодек должен знать свои ошибки:
производный `Codec.json (schema E)` знает, как и `Codec.transform_as
(schema E)` для своего тела. Это нужно ошибкам с общим телом, как у RealWorld
`{"errors": {…}}`. Кодек простого `Codec.transform` принимает любое значение,
и эндпоинт с таким вариантом среди других отклоняется при создании: его
ошибки нельзя различить.

### Аутентификация

`HttpApi.with_auth scheme Service check` строит для обработчика сервис
`Service` по каждому запросу, после хуков: например, пользователя, которого
называет заголовок `authorization`. `check` читает учётные данные из
заголовков, которые объявляет эндпоинт, и решает. Её типизированная ошибка
отправляется так же, как ошибка обработчика, со статусом и телом своего
варианта, а обработчик не запускается. У каждого запроса свой сервис, поэтому
одновременные запросы не видят чужих. Документ указывает для эндпоинта
`scheme`, но, в отличие от `with_security`, сервер сам 401 не отвечает: как
выглядит отказ, решает `check`. `with_optional_auth` обслуживает и анонимные
запросы, а документ указывает схему как необязательную.
`HttpApi.authenticated Service group` убирает сервис из того, что нужно для
обслуживания группы; эндпоинт группы без `with_auth` для него — дефект при
создании группы.

```flick
service Session =
  user : Unit ->{} Option String

type alias Credentials = { authorization : Option String }
  deriving Schema

check : HttpApi.Request Unit Unit Credentials Unit ->{; Unauthorized} Session.Impl
let check request =
  match request.headers.authorization with
  | Some "Token ada" -> { user = fun () -> Some "ada" }
  | Some _ -> from_result (Err (Unauthorized { errors = { token = ["is invalid"] } }))
  | None -> from_result (Err (Unauthorized { errors = { token = ["is missing"] } }))

me : HttpApi.Endpoint "me" (HttpApi.Request Unit Unit Credentials Unit ->{Session; Unauthorized} String)
let me =
  HttpApi.endpoint HttpApi.Get "/me" (Record.label : Record.Label "me")
  |> HttpApi.with_headers credentials
  |> HttpApi.with_success 200 (Codec.json Schema.string)
  |> HttpApi.with_errors (HttpApi.errors |> HttpApi.error_case 401 (Codec.json (schema Unauthorized)))
  |> HttpApi.with_auth token Session check
```

Здесь `credentials` декодирует запрос без заголовка как `None`
(`optional_field "authorization" String.json`), а `token` —
`HttpApi.api_key "Token" { key = "authorization"; location = HttpApi.InHeader }`.
Оба таргета обслуживают это одинаково.

## Обслуживание запросов

```flick
let server = HttpApi.listen options api { users = { get_user = find_user } }
```

`listen` требует `Scope s` и обслуживает запросы до закрытия этой области.
При закрытии сервер перестаёт принимать новые запросы, даёт завершиться
выполняющимся и запускает их финализаторы.

Если обработчик отсутствует или указан лишний, возникает ошибка типов
с именами эндпоинта и группы
(`field \`ping\` is missing from \`users\``). Если часть запроса не удалось
декодировать, сервер отвечает статусом 400 с `{ error, issues }`.
Для каждой проблемы передаются путь и сообщение, но не входные данные.

`options` задаёт хост и порт (0 — любой свободный порт), а также пути
для документа OpenAPI, Swagger UI и Scalar. Для них используется тот же
документ, который возвращает `HttpApi.openapi`.

`HttpApi.route "/api/*" api handlers` обслуживает API на сервере
`Http.Router`: один маршрут по этому шаблону пути рядом со страницами и
вызовами `server fn`, на одном origin. Он декодирует и отвечает так же, как
`listen`, на запрос, который не принял ни один эндпоинт, отвечает 404, а
обработчики работают с сервисами того места, где создан маршрут.

Собранное одностраничное приложение и его API так же делят один сервер, на
обоих таргетах: маршрут API рядом с `Http.StaticServer.route` с `spa = true`,
который на путь без файла отвечает `index.html`, если запрос принимает HTML
(собственные маршруты клиента), и `Http.Middleware.cors` вокруг маршрутизатора
для dev-сервера на другом origin.

```flick
let files = Http.StaticServer.route { Http.StaticServer.static_options "dist" with spa = true } ""
let app =
  Http.Middleware.cors
    { Http.Middleware.cors_options with allowedOrigins = ["http://localhost:5173"] }
    (Http.Router.router Http.Router.config [HttpApi.route "/api/*" api handlers, files])
let server = Http.Server.serve options app
```

## Клиенты и документы

`HttpApi.call base endpoint request` кодирует запрос кодеками эндпоинта
и декодирует ответ. Вызов возвращает успешный результат либо
завершается объявленной типизированной ошибкой или `HttpApi.ClientError`.
`HttpApi.url` строит только URL.

`HttpApi.openapi api` возвращает документ OpenAPI 3.1. Каждую схему
в нём записывает строгий экспортёр. Если какую-либо часть нельзя представить
точно, экспорт завершается ошибкой `ExportError` с путём
`group.endpoint.part -> field`.
Команда `flick schema export Module.api --target openapi-3.1 --out openapi.json`
записывает тот же документ.
