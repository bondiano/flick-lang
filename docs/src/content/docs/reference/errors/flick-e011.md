---
title: "FLICK-E011"
---

A `server fn` that lists `Server.Unauthorized` among its errors is guarded:
its implementation is a `Server.Guarded`, and only `Server.authenticated`
serves it, with the service its check makes from the headers of the call.
`Server.handler` takes it only through that. The other way round, a server
fn that `Server.authenticated` serves must list `Server.Unauthorized`, so its
callers know a call can be turned away.

```flick ignore
whoami : Unit ->{Server.Server, Session; Server.Unauthorized} String
server fn whoami () = Session.user ()

routes : forall s. Unit ->{Scope s, Server.Server;} (ServerRequest ->{Async;} ServerResponse)^{s}
let routes () = Server.handler (Server.authenticated Session check [server whoami])
```
