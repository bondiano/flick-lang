---
title: "Typed HTTP APIs"
---

`HttpApi` describes an HTTP API as values: endpoints with the codecs of
their parts, groups of endpoints and an API of groups (see [The boundary of the language core](../../spec/05-errors-and-concurrency/#the-boundary-of-the-language-core)). The type of an API is the record of handlers
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
    |> HttpApi.with_path (Codec.json UserPath.schema)
    |> HttpApi.with_success 200 (Codec.json User.schema)
    |> HttpApi.with_errors (HttpApi.errors |> HttpApi.error_case 404 (Codec.json NotFound.schema))

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

Of several cases, the server answers an error with the first case whose
codec has it, so each codec must know its errors: a derived `Codec.json
E.schema` does, and so does `Codec.transform_as E.schema` for a body of
your own. Errors that share a body, as RealWorld's `{"errors": {…}}`, need
it. A codec of a plain `Codec.transform` takes any value, and an endpoint with
such a case among others is refused when it is made, as its errors cannot
be told apart.

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
# type error Unauthorized = Unauthorized { errors : { token : List String } }
#   deriving Schema
# import App.Auth exposing (credentials, token)
#
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
  |> HttpApi.with_errors (HttpApi.errors |> HttpApi.error_case 401 (Codec.json Unauthorized.schema))
  |> HttpApi.with_auth token Session check
```

Here `credentials` decodes a request without the header as `None`
(`optional_field "authorization" String.json`), and `token` is
`HttpApi.api_key "Token" { key = "authorization"; location = HttpApi.InHeader }`.
Both targets serve it alike.

### An API as a type

A record that derives `HttpApi` is a group whose fields are its endpoints,
written with the types of `HttpApi.Derive`:

```flick
import HttpApi.Derive exposing (..)
# type alias Credentials = { authorization : Option String }
#   deriving Schema
# type alias Name = { name : String }
#   deriving Schema
# type alias Profile = { name : String; bio : String }
#   deriving Schema
# type alias Note = { body : String }
#   deriving Schema
# type error Unauthorized = Unauthorized
#   deriving Schema
# instance Problem Unauthorized =
#   let problem = { status = 401; body = Codec.json Unauthorized.schema }
# type error NotFound = NotFound
#   deriving Schema
# instance Problem NotFound =
#   let problem = { status = 404; body = Codec.json NotFound.schema }

type alias Users = {
  me : Authed (Get "/me" { headers : Credentials } String {Unauthorized});
  profile : MaybeAuthed (Get "/profiles/:name" { headers : Credentials; path : Name } Profile {Unauthorized, NotFound});
  post : Authed (Post "/notes" { headers : Credentials; payload : Note } (Created Note) {Unauthorized});
} deriving HttpApi
```

- The field names the endpoint, and the type's name in snake case the group
  (`users`).
- `Get`, `Post`, `Put`, `Patch` and `Delete` take the path, the request as a
  record of the parts it has (`path`, `query`, `headers`, `payload`) or `{}`,
  the success and the errors. Each `:name` of the path is a field of the
  `path` part. The path is a string, or a constructor of a route type that
  derives `Url`, declared in another module, with the type of each
  parameter in place of a value: `Post (Profiles Username (Some Follow))`
  is `/profiles/:username/follow` (see
  [Routes as the paths of an API](../route/#routes-as-the-paths-of-an-api)).
- The success is sent with 200, `Created a` with 201, and `Unit` with 204
  and no body.
- A part or a success is decoded by the value named after its type in snake
  case in the type's module (`credentials` for `Credentials`) when that is a
  `PureCodec Json` of the type, a refined type's `T.json`, a built-in type's
  `String.json`, or else the JSON codec of its schema.
- Each error has an `instance Problem E = let problem = { status = 404; body = … }`.

The type has two companions: `Users.endpoints`, the record of its endpoints,
which `HttpApi.call` requests, and `Users.api`, the group. With `Authed` and
`MaybeAuthed` endpoints, `Users.api` takes the security scheme, the service
and its two checks, required and optional, and applies `with_auth`,
`with_optional_auth` and `authenticated` as above; every endpoint of such a
group is `Authed` or `MaybeAuthed`, and they read the same `headers`. A
check gets only the headers, `HttpApi.Request Unit Unit Credentials Unit`,
and fails with errors every such endpoint declares.

```flick
# import HttpApi.Derive exposing (..)
# import App.Auth exposing (token, check, maybe)
# service Session =
#   user : Unit ->{} String
# type alias Credentials = { authorization : Option String }
#   deriving Schema
# type error Unauthorized = Unauthorized
#   deriving Schema
# instance Problem Unauthorized =
#   let problem = { status = 401; body = Codec.json Unauthorized.schema }
# type alias Users = {
#   me : Authed (Get "/me" { headers : Credentials } String {Unauthorized});
# } deriving HttpApi
# type alias Public = {
#   ping : Get "/ping" {} String {};
# } deriving HttpApi
#
# let example = task {
let api =
  HttpApi.api "Notes"
  |> HttpApi.add_group (Users.api token Session check maybe)
  |> HttpApi.add_group Public.api
# ()
# }
```

`HttpApi.Mount.under "/api" api` puts every endpoint under a prefix, as the
server serves and documents it; a client then calls with the prefix in its
base, `HttpApi.call "/api" Users.endpoints.me request`.

std's plugin writes both companions in Flick, with `HttpApi.Derive.at`
and the functions above, so the checker finds what does not fit, on the
endpoint's field: a record it cannot read as endpoints is `std/D011`, a part
or a success with no codec `FLICK-D011`, an error without a `Problem`
instance or a path whose parameters are not the fields of its `path` part
`FLICK-C001`.

## Serving

```flick
# import App.Users exposing (api, options, find_user)
#
# let example = task {
let server = HttpApi.listen options api { users = { get_user = find_user } }
# ()
# }
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

A built single-page app and its API share one server the same way, on both
targets: the API's route beside `Http.StaticServer.route` with `spa = true`,
which answers a path of no file with `index.html` when the request accepts
HTML (the client's own routes), and `Http.Middleware.cors` around the router
for a dev server on another origin.

```flick
# import App.Notes exposing (api, handlers, options)
#
# let example = task {
let files = Http.StaticServer.route { Http.StaticServer.static_options "dist" with spa = true } ""
# app : Http.Middleware.Handler {Async} {Http.ServerError.HttpServerError}
let app =
  Http.Middleware.cors
    { Http.Middleware.cors_options with allowedOrigins = ["http://localhost:5173"] }
    (Http.Router.router Http.Router.config [HttpApi.route "/api/*" api handlers, files])
let server = Http.Server.serve options app
# ()
# }
```

## Clients and documents

`HttpApi.call base endpoint request` encodes the request with the endpoint's
codecs and decodes the response: the success, a declared error as the
typed failure, or `HttpApi.ClientError`. `HttpApi.url` builds the URL alone.
In a browser the base may be relative, `/api`, against the page's address;
elsewhere a relative base is a `ClientError` on JS.

`HttpApi.Inject.call handler base endpoint request` is `call` answered by a
handler of the server in the same process, as Fastify's `inject`: the
server's router, `Http.Router.router config [HttpApi.route "/api/*" api
handlers]`, gets the request a client would send and answers it with the
same routing, auth, decoding and errors, so a test checks the whole server
without a socket; an error the router does not answer is answered as a
server would, a missing route with 404.

`HttpApi.openapi api` is the OpenAPI 3.1 document, every schema written by
the strict exporter; a part without an exact form is an `ExportError`
naming `group.endpoint.part -> field`. `flick schema export Module.api
--target openapi-3.1 --out openapi.json` writes the same document.
