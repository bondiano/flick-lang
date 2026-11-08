---
title: "FLICK-E011"
source_hash: "3b76b3efc8e7890a"
---

`server fn`, который перечисляет `Server.Unauthorized` среди своих ошибок, защищён:
его реализация — это `Server.Guarded`, и обслуживает его только `Server.authenticated`,
сервис которого проверяет его по заголовкам вызова.
`Server.handler` принимает его только таким образом. Обратно, серверная
функция, которую обслуживает `Server.authenticated`, должна перечислять `Server.Unauthorized`,
чтобы её вызывающие знали, что вызов может быть отклонён.

```flick ignore
whoami : Unit ->{Server.Server, Session; Server.Unauthorized} String
server fn whoami () = Session.user ()

routes : forall s. Unit ->{Scope s, Server.Server;} (ServerRequest ->{Async;} ServerResponse)^{s}
let routes () = Server.handler (Server.authenticated Session check [server whoami])
```
