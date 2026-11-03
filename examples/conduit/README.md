# Conduit in Flick

[RealWorld](https://github.com/realworld-apps/realworld)'s Conduit, a Medium
clone, written end to end in Flick: a Go server over PostgreSQL and a
single-page client in the browser, which share one typed contract of the
API. It passes the official RealWorld suites as they are vendored in
[`vendor/realworld`](vendor/realworld/SOURCE.md): the Hurl suite of the API
and the Playwright suite of the frontend.

It needs no foreign bindings: the only JavaScript is the page's entry
script and the Vite and Playwright configs, and the only SQL is in the
checked `sql` blocks.

```
contract/   the API as types: endpoints, wire forms, errors (a lib, built for js and go)
server/     the API on the go target, over PostgreSQL
client/     the pages on the js target, built with Vite
vendor/     the RealWorld spec, theme and test suites, unchanged
```

## Running it

You need Docker, Go, Node with pnpm, and a `flick` built from this
repository.

```sh
just conduit-run          # builds both, starts PostgreSQL and the server: http://127.0.0.1:8000
```

From this directory, `flick run <name>` runs the steps of
[`flick.toml`](flick.toml):

| Script | What it does |
| - | - |
| `lint` | `flick fmt --check` and `flick check --warnings-as-errors` over all three packages, under tight `[lint]` thresholds |
| `test` | every package's tests; the server's API suite starts its own PostgreSQL in Docker |
| `build` | builds every package |
| `db`, `migrate` | a PostgreSQL on port 54329, and the migrations applied to it |
| `api` | the RealWorld Hurl suite against a fresh server and database (needs `hurl`) |
| `e2e` | the RealWorld Playwright suite against the client as the server serves it (needs Chromium: set `CHROMIUM`, or `pnpm --dir client exec playwright install chromium`) |
| `ci` | all of the above |

To work on the client, run the server (`flick run --package server main`
with `DATABASE_URL` and `JWT_SECRET`) and `pnpm --dir client dev`: Vite rebuilds
a changed module and keeps the page's state, and sends `/api` to the
server on `API_PORT` (8000).

## A tour

### One contract, two targets

[`contract`](contract/flick.toml) is a source package. The workspace's
packages depend on it by path, and each compiles it for its own target, so
the client and the server agree on every type by construction.

### Every path, one type

The paths of the API are values of one type that derives `Url`
([`Conduit.Route`](contract/src/Conduit/Route.fl)). A last field of
another route type is the rest of the path, so the type covers the whole
REST surface:

```flick
type ApiRoute =
  | Users (Option UserAction)                 // /users, /users/login
  | User                                      // /user
  | Profiles Username (Option ProfileAction)  // /profiles/:username[/follow]
  | Articles (Option ArticleRoute)            // /articles, /articles/feed, /articles/:slug/…
  | Tags
  deriving Eq, Show, Schema, Url
```

The endpoints ([`Conduit.Api`](contract/src/Conduit/Api.fl)) are records
that derive `HttpApi`. Each names its path with those constructors, the
type of each parameter in place of a value, its request parts, its answer
and the errors it may answer with:

```flick
type alias Users = {
  follow : Authed
    (Post
      (Profiles Username (Some Follow))
      { headers : Credentials; path : ProfilePath }
      ProfileBody
      {Unauthorized, NotFound});
  …
} deriving HttpApi
```

`Profiles Username (Some Follow)` is `/profiles/:username/follow`; a
misspelt path is a type error. `Authed` and `MaybeAuthed` say which
endpoints need a user. The server mounts the whole API under `/api`
(`HttpApi.Mount.under`), and its OpenAPI document comes from the same
value:

```sh
flick schema export Conduit.Server.Routes.api --target openapi-3.1 --out openapi.json
```

In an editor, `Api.` after `import Conduit.Api as Api` lists the groups,
`Api.Public.` their companions, and hovering `Public` shows its doc.

### Typed errors

Each error of the API is a type with a
[`Problem`](contract/src/Conduit/Problem.fl) instance: its status and the
RealWorld body. A handler fails with `Unprocessable problems`, and a
client catches the same constructor:

```flick
try (Session.call request Api.Public.endpoints.login).user with
| Unauthorized found -> …
| Unprocessable found -> …
```

### The schema is its migrations

[`Conduit.Server.Schema`](server/src/Conduit/Server/Schema.fl) holds the
database as `sql migration` declarations. Each table they leave is a type
of the module, so the typed queries read exactly the columns the
migrations make, and `migrations` is what `Database.Migrate.run` applies
when the server starts:

```flick
sql migration 1 "conduit" {
  create table users (
    id bigint primary key generated always as identity,
    username text not null unique,
    …
  );
  …
}
```

A query names tables and columns of that schema
([`Conduit.Server.Articles`](server/src/Conduit/Server/Articles.fl)), with
joins, aggregates and subqueries; its row type is checked against what it
selects.

### Services, not globals

The server's handlers say what they need in their types:
`{Session, Database, Crypto.Crypto | r; Unauthorized, NotFound}`.
[`Conduit.Server.Main`](server/src/Conduit/Server/Main.fl) reads its
settings with `Config`, provides the database, the signing key and the
system's cryptography as layers, and serves
[`Conduit.Server.App`](server/src/Conduit/Server/App.fl). Passwords are
PBKDF2 (`Crypto.hash_password`), tokens HS256 JWTs (`Jwt`), and the
validation of every request is a pure module
([`Validate`](server/src/Conduit/Server/Validate.fl)) with tests of its own.

### End-to-end tests in Flick

[`ApiTest`](server/test/Conduit/Server/ApiTest.fl) tests the API as
Fastify's `inject` does: typed requests of the contract's endpoints go to
the server's own router in the same process, `HttpApi.Inject.call`, and
come back as typed answers. The tests are a `Test.suite` whose layer is
its `beforeAll` and `afterAll`:
[`Postgres`](server/test/Conduit/Server/Postgres.fl) starts PostgreSQL in
Docker from Flick with `ChildProcess`, migrates it, and stops it after the
last test.

```flick
tests : List Test.Test
let tests = [Test.suite "the API over PostgreSQL" services (accounts ++ publishing)]
```

### The client

[`Conduit.Client`](client/src/Conduit/Client/Main.fl) is the RealWorld
frontend on the js target, rendered with React:

- The pages are a type that derives `Url`
  ([`Route`](client/src/Conduit/Client/Route.fl)), so `/profile/ada/favorites`
  is `Profile "ada" (Some Favorites)` and a link names its page.
  `Atom.route` holds the page of the address bar; writing it navigates.
- Data is queries keyed by the reader and the page (`Atom.family`,
  `Atom.query`), which mutations of the same keys load again.
- Forms bind their inputs to states (`Ui.field`), and actions
  (`Action.exhaust`) send them as typed requests.
- The token lives in `Atom.stored "jwtToken"`, so a session survives a
  reload.
- Article bodies go through `Markdown.to_html`, which escapes every text:
  the spec's XSS tests pass by construction.

## Tests at a glance

| Suite | Where | Runs |
| - | - | - |
| unit | `contract/test`, `server/test/…/CoreTest.fl`, `client/test` | `flick test` |
| API, in process | `server/test/…/ApiTest.fl` | `flick test` (Docker) |
| API, over HTTP | RealWorld Hurl | `flick run api`, `just conduit-api` |
| frontend | RealWorld Playwright, `fullstack` mode | `flick run e2e`, `just conduit-e2e` |
