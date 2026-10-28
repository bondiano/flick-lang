---
title: "Типизированные HTTP API"
---

`HttpApi` позволяет описать HTTP API значениями программы: эндпоинты
с кодеками параметров, заголовков и тела, группы эндпоинтов и API
из этих групп (см. [Граница языкового ядра](../../spec/07-errors-and-concurrency/#граница-языкового-ядра)).
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
| `describe`, `deprecate`, `hide` | добавляют краткое описание, пометку об устаревании или исключают эндпоинт из документа |

Эндпоинт создаётся без типизированных ошибок: обработчик может завершиться
только ошибкой, для которой `with_errors` задал вариант. Обработчик
с ошибкой без варианта, например новой `Conflict`, — ошибка типов ещё
до запуска. `HttpApi.HttpError { status; message }` отправляется
с указанным в ней статусом там, где её объявляет `http_error status`.
Обработчик потоковой формы может завершиться и ошибкой потока
`MultipartError`; она отправляется с указанным в ней статусом.

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
