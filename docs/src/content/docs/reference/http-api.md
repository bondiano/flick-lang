---
title: "Typed HTTP APIs"
---

`HttpApi` describes an HTTP API as values: endpoints with the codecs of
their parts, groups of endpoints and an API of groups (see [The boundary of the language core](../../spec/07-errors-and-concurrency/#the-boundary-of-the-language-core)). The type of an API is the record of handlers
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
    |> HttpApi.with_errors (HttpApi.errors |> HttpApi.error_case 404 (Codec.json (schema NotFound)))

api : HttpApi.Api r { users : { get_user : HttpApi.Request UserPath Unit Unit Unit ->{r; NotFound} User } }
let api =
  HttpApi.api "Users"
    |> HttpApi.add_group (HttpApi.group (Record.label : Record.Label "users") |> HttpApi.add get_user)
```

## Endpoints

`HttpApi.endpoint method path label` starts an endpoint whose handler takes
`Request Unit Unit Unit Unit` (`{ path; query; headers; payload }`) and
answers 204. The signature of the endpoint names the handler's type: its
services `r` stay open, and its errors `e` are the typed errors the handler
may fail with. A group and an API carry the services of all their handlers,
`HttpApi.Group n r hs` and `HttpApi.Api r gs`, and `listen` needs them: a
handler that asks for a service no layer provides there does not compile.

| Combinator | |
| - | - |
| `with_path codec` | path parameters (`/users/:id`), decoded by a record codec from their texts |
| `with_query codec` | the query |
| `with_headers codec` | headers, by lower-case name; headers the codec does not name are ignored |
| `with_payload codec` | a JSON body |
| `with_text_payload`, `with_form_payload codec` | a plain-text body, a URL-encoded form |
| `with_success status codec`, `with_text_success status` | the status and body of a success |
| `with_errors (errors \|> error_case status codec \|> …)` | the typed errors the handler may fail with, and how each is sent |
| `with_security scheme` | `bearer name`, `basic name` or `api_key name { key; location }`: 401 without credentials |
| `with_middleware hook` | a runtime function on the decoded request before the handler |
| `with_auth scheme Service check`, `with_optional_auth …` | a service for the handler that `check` makes from each request (below) |
| `describe`, `deprecate`, `hide` | the summary, deprecation, absence from the document |

An endpoint starts with no typed error: its handler fails only with the
errors `with_errors` gives a case, so a handler failing with an error no
case has, a new `Conflict` say, is a type error before anything runs.
`HttpApi.HttpError { status; message }` is sent with its own status where
`http_error status` declares it. A streamed form's handler may also fail
with the stream's `MultipartError`, sent with the status it names.

### Authentication

`HttpApi.with_auth scheme Service check` makes the service `Service` for the
handler from each request, after the hooks: the user its `authorization`
header names, say. `check` reads the credentials from the headers the endpoint
declares and decides. Its typed failure is answered as one of the handler's
would be, with the status and body of its error case, and the handler does
not run. Each request has its own service, so concurrent requests do not see
each other's. The document lists `scheme` for the endpoint, but unlike
`with_security` the server does not answer 401 by itself: what a refusal
looks like is up to `check`. `with_optional_auth` also serves anonymous
requests, and the document lists the scheme as optional. `HttpApi.authenticated
Service group` takes the service off what serving the group needs; an endpoint
of the group without `with_auth` for it is a defect when the group is made.

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

Here `credentials` decodes a request without the header as `None`
(`optional_field "authorization" String.json`), and `token` is
`HttpApi.api_key "Token" { key = "authorization"; location = HttpApi.InHeader }`.
Both targets serve it alike.

## Serving

```flick
let server = HttpApi.listen options api { users = { get_user = find_user } }
```

`listen` needs `Scope s` and serves until that scope closes: then it stops
accepting, lets the requests in flight finish and runs their finalizers.
A missing or extra handler is a type error naming the endpoint and its
group (`field \`ping\` is missing from \`users\``). A request whose parts do
not decode is answered 400 with `{ error, issues }` — the path and message of
each issue, never the input. `options` give the host and port (0 for any) and the paths of the
OpenAPI document, Swagger UI and Scalar, which all serve the document
`HttpApi.openapi` writes.

`HttpApi.route "/api/*" api handlers` serves the API on a server of
`Http.Router` instead: one route at that path pattern, beside the pages and
the calls of `server fn`, on one origin. It decodes and answers as `listen`
does, a request no endpoint takes is 404, and the handlers run with the
services where the route is made.

## Clients and documents

`HttpApi.call base endpoint request` encodes the request with the endpoint's
codecs and decodes the response: the success, a declared error as the
typed failure, or `HttpApi.ClientError`. `HttpApi.url` builds the URL alone.

`HttpApi.openapi api` is the OpenAPI 3.1 document, every schema written by
the strict exporter; a part without an exact form is an `ExportError`
naming `group.endpoint.part -> field`. `flick schema export Module.api
--target openapi-3.1 --out openapi.json` writes the same document.
