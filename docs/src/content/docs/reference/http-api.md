---
title: "Typed HTTP APIs"
---

`HttpApi` describes an HTTP API as values: endpoints with the codecs of
their parts, groups of endpoints and an API of groups (spec §7 "Граница
языкового ядра", roadmap M21). The type of an API is the record of handlers
it needs, so serving it takes exactly one handler per endpoint; the client
requests an endpoint from the same value, and the strict JSON Schema
exporter writes the API as OpenAPI 3.1.

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

## Endpoints

`HttpApi.endpoint method path label` starts an endpoint whose handler takes
`Request Unit Unit Unit Unit` (`{ path; query; headers; payload }`) and
answers 204. The signature of the endpoint names the handler's type: its
services `r` stay open, and its errors `e` are the typed errors the handler
may fail with.

| Combinator | |
| - | - |
| `with_path codec` | path parameters (`/users/:id`), decoded by a record codec from their texts |
| `with_query codec` | the query |
| `with_headers codec` | headers, by lower-case name; headers the codec does not name are ignored |
| `with_payload codec` | a JSON body |
| `with_text_payload`, `with_form_payload codec` | a plain-text body, a URL-encoded form |
| `with_success status codec`, `with_text_success status` | the status and body of a success |
| `with_errors [error_case status codec, …]` | how each typed error is sent |
| `with_security scheme` | `bearer name`, `basic name` or `api_key name { key; location }`: 401 without credentials |
| `with_middleware hook` | a runtime function on the decoded request before the handler |
| `describe`, `deprecate`, `hide` | the summary, deprecation, absence from the document |

`HttpApi.HttpError { status; message }` is always sent with its own status;
`http_error status` declares it for the document and the client.

## Serving

```flick
let server = HttpApi.listen options api { users = { get_user = find_user } }
```

`listen` needs `Scope s` and serves until that scope closes: then it stops
accepting, lets the requests in flight finish and runs their finalizers.
A missing or extra handler is a type error naming the endpoint and its
group (`field \`ping\` is missing from \`users\``). A request whose parts do
not decode is answered 400 with `{ error, issues }` — the path and message of
each issue, never the input. An error no case declares is 500 and is
logged. `options` give the host and port (0 for any) and the paths of the
OpenAPI document, Swagger UI and Scalar, which all serve the document
`HttpApi.openapi` writes.

## Clients and documents

`HttpApi.call base endpoint request` encodes the request with the endpoint's
codecs and decodes the response: the success, a declared error as the
typed failure, or `HttpApi.ClientError`. `HttpApi.url` builds the URL alone.

`HttpApi.openapi api` is the OpenAPI 3.1 document, every schema written by
the strict exporter; a part without an exact form is an `ExportError`
naming `group.endpoint.part -> field`. `flick schema export Module.api
--target openapi-3.1 --out openapi.json` writes the same document.
