---
title: "Server functions"
---

A `server fn` is a function that runs on the server and that any code,
the browser included, calls by its name. The compiler makes the endpoint,
the client call and the codecs of what crosses the network from one
declaration, so the two sides cannot disagree.

```flick
module App.Shop exposing (Order, OutOfStock, place)

type alias Order = { item : String; count : Number }
  deriving Schema

type error OutOfStock = OutOfStock String
  deriving Schema

/// Up to three of an item.
place : Order ->{Server.Server, App.Db.Db; OutOfStock} String
server fn place order =
  if order.count > 3 then from_result (Err (OutOfStock order.item))
  else App.Db.save order
```

The signature right above `server fn` is what the server runs: its
parameters, the services it needs and its errors. Only the last arrow runs:
`String -> Number ->{Server.Server;} String` takes two parameters. Callers see another type:

```flick
place : Order ->{; Rpc.ClientError, OutOfStock} String
```

The call needs no service, so a client entry may reach it
(`target = "client"`, see `FLICK-E009`), and it
is a task like any other: `Atom.task (task { place order })`, or the body of
an action. A declared error comes back as itself; `Rpc.ClientError` says
the call did not reach the server or its answer did not decode. A call goes
to the page's origin in a browser and to `FLICK_SERVER_URL` elsewhere, at
`Server.path` (`/_flick/fn`).

Secrets stay on the server the same way: `Config.redacted` needs
`Server.Server`, so a client entry that would read a password or a key is
`FLICK-E009`, and neither its name nor its value is in the page's bundle.

While `Server.handler` serves a function in a process, a call of it made
in that process, as a page rendered on the server makes, runs in memory:
with the handler's services, the headers of the request being served, and
what crosses written and read by its codecs, as over HTTP. A task an atom
runs while the page renders is part of the request's trace too.

## Serving

`server place` is the implementation, a `Server.Function r` whose row `r`
holds `Server` and what the body needs. `Server.handler` serves a list of
them on one route; a client entry that names `server place` reaches
`Server` and is refused.

```flick
serve : forall s. Unit ->{Scope s, Server.Server, App.Db.Db;} (ServerRequest ->{} ServerResponse)^{s}
let serve () = Server.handler [server App.Shop.place]
```

Mount the handler on `Http.Router` as a POST of `Server.path`:

```flick
let route functions =
  Http.Router.route (Some Http.Method.POST) Server.path (fun request -> functions request)
```

## What crosses the network

The payload (the parameter, or a tuple of several), the success and each
error travel as JSON, so each has a
Schema witness: a record or tuple of witnessed types, `List`, `Option`, a
type that derives `Schema`. The compiler checks this where the function is
declared (`FLICK-E010`), together with the rest of the shape: pure arrows
before the last one, each parameter named, no type variables, the errors
listed.

## A public endpoint

Every `server fn` is a public endpoint: anyone who reaches the server can
call it with any payload, not only the program's own pages. `flick doc`
marks each one so.

## Who calls

A function that lists `Server.Unauthorized` among its errors is guarded:
its implementation is a `Server.Guarded`, which `Server.handler` takes only
through `Server.authenticated`. That runs a check on the headers of each
call and gives the function the service the check makes; a check that
fails with `Server.Unauthorized` answers the call with it, and the body
does not run. Serving a guarded function without `Server.authenticated`, or
guarding one that does not list `Server.Unauthorized`, is `FLICK-E011`.

```flick
service Session =
  user : Unit ->{} String

whoami : Unit ->{Server.Server, Session; Server.Unauthorized} String
server fn whoami () = Session.user ()

check : Http.Headers.Headers ->{; Server.Unauthorized} Session.Impl
let check headers =
  match Http.Headers.get "cookie" headers with
  | Some cookie -> { user = fun () -> cookie }
  | None -> from_result (Err Server.Unauthorized)

let functions =
  Server.handler (Server.authenticated Session check [server whoami] ++ [server open_door])
```

The caller handles `Server.Unauthorized` as any declared error. Each call
has its own session, so concurrent callers do not see each other's. A call
made in memory while a page renders carries the headers of the page's
request, cookie included. Passing a guarded function to `Server.handler`
directly is a type error, and so is a function that needs `Session`
without listing `Server.Unauthorized` under `authenticated`: the session
would be left to whoever serves it.
