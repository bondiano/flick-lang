---
title: "Типизированные HTTP API"
---

`HttpApi` описывает HTTP API как значения: конечные точки с кодеками их
частей, группы конечных точек и API из групп (спецификация §7 «Граница
языкового ядра», дорожная карта M21). Тип API — запись необходимых ему
обработчиков, поэтому для его обслуживания требуется ровно один обработчик
на конечную точку; клиент обращается к конечной точке по тому же значению,
а строгий экспортёр JSON Schema записывает API как OpenAPI 3.1.

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
    |> HttpApi.with_errors [HttpApi.error_case 404 (Codec.json (schema NotFound))]

api : HttpApi.Api { users : { get_user : HttpApi.Request UserPath Unit Unit Unit ->{r; NotFound} User } }
let api =
  HttpApi.api "Users"
    |> HttpApi.add_group (HttpApi.group (Record.label : Record.Label "users") |> HttpApi.add get_user)
```

## Конечные точки

`HttpApi.endpoint method path label` создаёт начальную конечную точку,
обработчик которой принимает `Request Unit Unit Unit Unit`
(`{ path; query; headers; payload }`) и отвечает кодом 204. Сигнатура
конечной точки задаёт тип обработчика: его сервисы `r` остаются открытыми,
а ошибки `e` — типизированные ошибки, которыми может завершиться обработчик.

| Комбинатор | |
| - | - |
| `with_path codec` | параметры пути (`/users/:id`), декодируемые кодеком записи из их текстовых значений |
| `with_query codec` | строка запроса |
| `with_headers codec` | заголовки по именам в нижнем регистре; заголовки, не названные кодеком, игнорируются |
| `with_payload codec` | тело JSON |
| `with_text_payload`, `with_form_payload codec` | текстовое тело, форма в URL-кодировке |
| `with_success status codec`, `with_text_success status` | статус и тело успешного ответа |
| `with_errors [error_case status codec, …]` | способ отправки каждой типизированной ошибки |
| `with_security scheme` | `bearer name`, `basic name` или `api_key name { key; location }`: 401 без учётных данных |
| `with_middleware hook` | функция среды выполнения над декодированным запросом перед обработчиком |
| `describe`, `deprecate`, `hide` | краткое описание, пометка об устаревании, исключение из документа |

`HttpApi.HttpError { status; message }` всегда отправляется со своим
статусом; `http_error status` объявляет её для документа и клиента.

## Обслуживание запросов

```flick
let server = HttpApi.listen options api { users = { get_user = find_user } }
```

`listen` требует `Scope s` и обслуживает запросы до закрытия этой области:
затем прекращает принимать новые, даёт завершиться выполняющимся запросам
и запускает их финализаторы. Отсутствующий или лишний обработчик — ошибка
типов, указывающая конечную точку и её группу
(`field \`ping\` is missing from \`users\``). Запрос, части которого не удалось
декодировать, получает ответ 400 с `{ error, issues }` — путём и сообщением
каждой проблемы, но никогда не входными данными. Ошибка, не объявленная
ни одним вариантом, даёт 500 и записывается в журнал. `options` задаёт
хост и порт (0 — любой), а также пути документа OpenAPI, Swagger UI и
Scalar; все они предоставляют документ, который записывает `HttpApi.openapi`.

## Клиенты и документы

`HttpApi.call base endpoint request` кодирует запрос кодеками конечной
точки и декодирует ответ: успешный результат, объявленную ошибку как
типизированный сбой либо `HttpApi.ClientError`. `HttpApi.url` строит
только URL.

`HttpApi.openapi api` — документ OpenAPI 3.1, каждая схема в котором
записана строгим экспортёром; часть без точного представления —
`ExportError`, указывающая `group.endpoint.part -> field`.
`flick schema export Module.api --target openapi-3.1 --out openapi.json`
записывает тот же документ.
