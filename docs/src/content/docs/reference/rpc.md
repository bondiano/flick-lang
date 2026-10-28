---
title: "Typed RPC"
---

Define a procedure once, then use its description on both sides of the
connection. `Rpc` associates a name with codecs for the request, result and
typed errors; a group carries the exact record type of its handlers.
A missing or extra handler is a type error, not a failed first request.

Sharing a description keeps client and server code aligned, but does not
version a deployment: independently deployed peers must still agree on the
wire contract, serialization and transport. Use [typed HTTP APIs](../http-api/)
when endpoints, status codes and OpenAPI are the public contract; use RPC
when named operations and typed calls are the interface.

The following module serves and calls its own HTTP API. Save the first
Flick block as `RpcDemo.fl`; [schemas and codecs](../schema/) explain the
values used at the boundary.

```flick
module RpcDemo exposing (main)

import Stream.Core exposing (Stream)

type alias User = { id : Float; name : String }
  deriving Schema

type error NotFound = NotFound { id : Float }
  deriving Schema

get_user : Rpc.Rpc "get_user" (Float ->{r; NotFound} User)
let get_user =
  Rpc.make (Record.label : Record.Label "get_user")
    |> Rpc.with_payload (Codec.json Schema.number)
    |> Rpc.with_success (Codec.json (schema User))
    |> Rpc.with_errors (Rpc.errors |> Rpc.error_case (Codec.json (schema NotFound)))

count_to : Rpc.Rpc "count_to" (Float ->{r;} Stream none none Float)
let count_to =
  Rpc.make (Record.label : Record.Label "count_to")
    |> Rpc.with_payload (Codec.json Schema.number)
    |> Rpc.with_stream (Codec.json Schema.number)

type alias Handlers r =
  {
    get_user : Float ->{r; NotFound} User;
    count_to : Float ->{r;} Stream none none Float;
  }

users : Rpc.Group r (Handlers r)
let users = Rpc.group |> Rpc.add get_user |> Rpc.add count_to

handlers : Handlers none
let handlers =
  {
    get_user = fun id ->
      if id == 1 then { id = 1; name = "Ada" } else from_result (Err (NotFound { id = id }));
    count_to = fun n -> Stream.from_list (List.range 1 (n + 1));
  }

main : Unit ->{; AppError} Unit
let main () =
  try
    scope s {
      let server = Rpc.listen Rpc.defaults users handlers
      let url = "http://127.0.0.1:" ++ show (Rpc.port server) ++ "/rpc"
      let user = Rpc.call url get_user 1
      Platform.stdout_line user.name
      let numbers = Stream.run_collect (Rpc.call_stream url count_to 3)
      Platform.stdout_line (show numbers)
    }
  with
  | Rpc.ServeError message -> App.fail ("serve: " ++ message)
  | Rpc.ClientError message -> App.fail ("client: " ++ message)
  | NotFound e -> App.fail ("not found: " ++ show e.id)
```

```sh
flick build RpcDemo.fl --out-dir dist/rpc-demo
node dist/rpc-demo/flick-main.js
```

The successful path prints `Ada` and `[1, 2, 3]`. `NotFound` remains a
typed domain failure across the connection; `ClientError` means a transport
or decoding failure. The enclosing scope owns the server, so leaving it
also releases the listener.

## Procedures

`Rpc.make label` starts a procedure named `l` whose handler takes and
answers `Unit`; the signature of the procedure names the handler's type:
its services `r` stay open, and its errors `e` are those it declares with
`with_errors`: none until then, so a handler failing with an error no case
has is a type error.

| Combinator | |
| - | - |
| `with_payload codec` | the request, coded as JSON |
| `with_success codec` | the answer of a call |
| `with_stream codec` | the answer as a stream, each element coded and sent as it comes; a failure of the handler fails the stream |
| `with_errors (errors \|> error_case codec \|> …)` | the typed errors the handler may fail with, and how each is sent |
| `with_middleware hook` | a hook before the handler on each call, see below |
| `uninterruptible` | the server runs the handler to its end even when its client goes away or interrupts the call: for a write that must not stop halfway |

## Serialization

`Rpc.Serialization` describes message encoding, independently of transport.
Start with NDJSON for streaming HTTP. WebSocket already preserves message
boundaries; TCP does not, so it requires a framed serialization. Serializer
and transport implementations are separate values: unused binary and
WebSocket support can be removed from a browser bundle.

| Value | Wire form |
| - | - |
| `ndjson` | JSON, one message a line; a stream's elements come as they are sent. The default |
| `json` | one JSON document; over HTTP the messages of a call come together, so a stream's elements arrive once it ends |
| `json_rpc` | JSON-RPC 2.0 documents, for peers that speak it |
| `ndjson_rpc` | JSON-RPC 2.0, one message a line |
| `binary` | Effect's schema-driven frames: smaller than JSON, but both ends must be Effect's, and it adds to a bundle |

## Serving

| Function | Transport |
| - | - |
| `listen options group handlers` | a POST per call over HTTP, at `options.path` |
| `listen_websocket …` | each client a WebSocket at `path` whose messages carry its calls (`ws://127.0.0.1:8080/rpc`); every serialization streams over it |
| `listen_tcp …` | each client a TCP connection to `host` and `port` (`tcp://127.0.0.1:8080`). Node only. Only a serialization that frames its messages (`ndjson`, `ndjson_rpc`, `binary`) fits a byte stream; another is a `ServeError`, since TCP may split a document across reads |
| `serve_stdio serialization group handlers` | answers the calls of the messages on stdin on stdout until stdin ends, as editors run language servers; nothing else may write to stdout meanwhile. Node only |

The `listen` functions run on Node and acquire a `Server s` in `Scope s`.
Closing that scope stops admission and closes calls and their finalizers.
Handlers use the services available where the server was started.
`Rpc.port server` returns the actual port, including a system-selected one
when `port = 0`. `Rpc.defaults` selects `127.0.0.1`, port `0`, `/rpc` and
`ndjson`; `ServeError` reports failure to start.

`serve_stdio` instead runs until stdin ends and needs the `Stdio` service:
provide `Stdio.node`. Reserve stdout exclusively for protocol messages;
write diagnostics to stderr.

### Beside other routes

`Rpc.handler serialization group handlers` serves the group as an HTTP
handler, a POST per call, that `Http.Router` mounts like any other route;
`handler_websocket` is the same for a `GET` route of WebSockets. The
handler holds the region `s` of the scope that made it, and so do the
routes and the router over it: one of them returned out of that scope is
`FLICK-G001`.

```flick
serve_both : Http.Server.ServeOptions -> Handlers none ->{
  Http.Platform.HttpPlatform,
  Http.Etag.Generator;
  Http.ServerError.ServeError
}
  Unit
let serve_both options handlers =
  scope s {
    rpc : (ServerRequest ->{} ServerResponse)^{s}
    let rpc = Rpc.handler Rpc.ndjson users handlers
    let app =
      Http.Router.router Http.Router.config
        [
          Http.Router.route (Some Http.Method.POST) "/rpc" (fun request -> rpc request),
          Http.Router.route (Some Http.Method.GET) "/hello/:name" hello,
        ]
    let server = Http.Server.serve options app
    Platform.stdout_line (Http.Server.format_address (Http.Server.address server))
  }
```

The Node servers live behind their own entry point: a browser program
that only calls procedures pulls in no Node HTTP or socket server.

## Calling

| Function | |
| - | - |
| `call url procedure payload` | the answer of a call: the success, a declared error as the typed failure, or `ClientError` |
| `call_with options url procedure payload` | as `call`, with the client's serialization and transport |
| `call_stream url procedure payload` | a stream procedure's elements as they come |
| `call_stream_with options url procedure payload` | as `call_stream`, with the client's serialization and transport |
| `connect options url` | one connection in a scope that calls share until it closes: a WebSocket stays open, HTTP keeps its connections alive |
| `call_on client procedure payload`, `call_stream_on …` | a call over a connected client |
| `spawn serialization command args` | starts a child process and connects to it over its stdin and stdout, which its `serve_stdio` answers; its stderr is the program's, and the end of the scope stops the child. Node only |

`CallOptions` names the serialization and the transport: `Rpc.http` (a
POST per call, the default), `Rpc.websocket` (the messages of calls over
the global WebSocket — only a program that names it bundles it) or
`Rpc.tcp` (a TCP connection opened on the first call, so a refused
connection is a `ClientError` of that call; Node only). Start from
`Rpc.call_defaults`. An `http://` URL serves `listen`, a `ws://` one
`listen_websocket`, a `tcp://` one `listen_tcp`.

## Middleware and headers

`Rpc.with_middleware hook` runs a hook before the handler on each call,
after the hooks the procedure already has, with the services of the
listen call. The hook sees the decoded payload and the headers of the
call — those of its HTTP request or WebSocket, with the names in lower
case — and its typed failure is answered as one of the handler's would
be, so it must be one of the procedure's declared errors. A function per
procedure replaces Effect's class-based middleware services.

`Rpc.with_headers headers task` runs `task` with its calls carrying the
headers too, which the hooks read: `authorization`, say.

The following hook checks that a header is present; it is not token
verification. Real authentication must validate the credentials before
allowing the handler to run.

```flick
type error Unauthorized = Unauthorized { reason : String }
  deriving Schema

require_authorization_header : Rpc.Call Unit ->{; Unauthorized} Unit
let require_authorization_header call =
  match Http.Headers.get "authorization" call.headers with
  | Some _ -> ()
  | None -> from_result (Err (Unauthorized { reason = "no authorization" }))
```

`Rpc.with_auth tag check` makes a service for the handler from each call,
after the hooks: the session an `authorization` header names, say. A
typed failure of `check` is answered as the handler's would be, and the
handler does not run; each call has its own service, so concurrent callers
do not see each other's. `Rpc.authenticated tag group` takes the service
off what serving the group needs; a procedure of the group without
`with_auth` for it is a defect when the group is made. Over a WebSocket the
headers are the connection's.

```flick
me : Rpc.Rpc "me" (Unit ->{Session; Unauthorized} String)
let me =
  Rpc.make (Record.label : Record.Label "me")
    |> Rpc.with_success (Codec.json Schema.string)
    |> Rpc.with_errors (Rpc.errors |> Rpc.error_case (Codec.json (schema Unauthorized)))
    |> Rpc.with_auth Session check

group : Rpc.Group none { me : Unit ->{Session; Unauthorized} String }
let group = Rpc.authenticated Session (Rpc.group |> Rpc.add me)
```

Use `Rpc.uninterruptible` only when a handler must finish after client
cancellation. It can delay server shutdown; it does not make a write atomic
or protect it from process termination. Transactionality still belongs in
the operation's implementation.

## Testing handlers

`Rpc.call_local group handlers procedure payload` calls a procedure in
memory, client and server joined without a transport, and
`call_stream_local` does the same for a stream. The codecs do not run,
so a test of them still needs a real server; the hooks of
`with_middleware` see no headers there.

In-memory calls are useful for handler logic and declared failures. Add
a real transport round trip when testing codec compatibility, headers or
stream framing. `Rpc` wraps Effect's unstable RPC modules; pin matching
compiler and runtime versions.
