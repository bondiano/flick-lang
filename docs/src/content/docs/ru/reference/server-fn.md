---
title: "Серверные функции"
source_hash: 1cd5c516da779fca
---

`server fn` — функция, которая выполняется на сервере и которую любой код,
в том числе браузерный, вызывает по имени. Это блок плагина `server` пакета
`std`, который файл импортирует из `Server`; из одного объявления плагин
пишет endpoint, клиентский вызов и кодеки всего, что идёт по сети, поэтому
стороны не могут разойтись.

```flick
module App.Shop exposing (Order, OutOfStock, place)

import Server
# import App.Db

type alias Order = { item : String; count : Float }
  deriving Schema

type error OutOfStock = OutOfStock String
  deriving Schema

/// Не больше трёх штук.
place : Order ->{Server.Server, App.Db.Db; OutOfStock} String
server fn place order =
  if order.count > 3 then Fail.fail (OutOfStock order.item)
  else App.Db.save order
```

Сигнатура прямо над `server fn` описывает то, что выполняет сервер:
параметры, нужные сервисы и ошибки. Выполняется только последняя стрелка:
`String -> Float ->{Server.Server} String` принимает два параметра. Вызывающий код видит другой тип:

```flick
# type alias Order = { item : String; count : Float }
# type error OutOfStock = OutOfStock String
#
place : Order ->{; Rpc.ClientError, OutOfStock} String
# let place order = order.item
```

Вызову не нужны сервисы, поэтому он допустим в коде, достижимом из клиентской точки входа
(`target = "client"`, см. `FLICK-E009`), и это
обычная задача: `Atom.task (task { place order })` или тело action.
Объявленная ошибка возвращается как есть; `Rpc.ClientError` значит, что
вызов не дошёл до сервера или ответ не декодировался. Вызов идёт по пути
`Server.path` (`/_flick/fn`) на origin ближайшего `Server.with_origin`, а
без него в браузере — на origin страницы (см.
[Вне браузера](#вне-браузера)).

Секреты остаются на сервере так же: `Config.redacted` требует
`Server.Server`, поэтому клиентский вход, который прочёл бы пароль или ключ,
получает `FLICK-E009`, и ни имени секрета, ни его значения нет в бандле
страницы.

Пока `Server.handler` обслуживает функцию в процессе, её вызов в том же
процессе (так делает страница, отрендеренная на сервере) выполняется в
памяти: с сервисами обработчика, заголовками обслуживаемого запроса и
кодеками для всего, что передаётся, как по HTTP. Задача атома, запущенная
при рендере страницы, тоже попадает в трейс запроса.

## Вне браузера

У программы, которая не страница, например у клиента командной строки или
теста, своего origin нет. `Server.with_origin` задаёт его вызовам задачи,
а вне его такой вызов завершается ошибкой `Rpc.ClientError`. Вызов ничего
не читает из окружения, поэтому программа, которая берёт адрес сервера из
переменной, читает её сама и указывает `Host.Environment` в своей строке
эффектов:

```flick
# import Server
# type alias Order = { item : String; count : Float }
# type error OutOfStock = OutOfStock String
#
# place : Order ->{; Rpc.ClientError, OutOfStock} String
# let place order = order.item
#
main : Unit ->{Host.Environment, Host.Console, Async; AppError} Unit
let main () =
  let origin = Option.with_default "http://127.0.0.1:3000" (Platform.env "FLICK_SERVER_URL")
  let placed =
    try Server.with_origin origin (task { place { item = "tea"; count = 1 } }) with
    | OutOfStock item -> "out of stock: " ++ item
    | Rpc.ClientError message -> message
  Platform.stdout_line placed
```

В браузере `Server.with_origin` отправляет вызовы на другой origin, не
страницы, и тогда нужен CORS (см. [На Go-target](#на-go-target)). Вызов,
который выполняется в памяти, остаётся в памяти и внутри него.

## Обслуживание

`server place` — реализация, `Server.Function r`, где строка `r` содержит
`Server` и все сервисы, необходимые реализации функции. `Server.handler` обслуживает список таких
функций на одном маршруте; использование `server place` в коде клиентской точки входа
требует `Server`, поэтому компилятор его отклоняет.

```flick
# import Http.ServerRequest exposing (ServerRequest)
# import Http.ServerResponse exposing (ServerResponse)
# import Server
# import App.Db
# import App.Shop
#
serve : forall s. Unit ->{Scope s, Server.Server, App.Db.Db} (ServerRequest ->{Async} ServerResponse)^{s}
let serve () = Server.handler [server App.Shop.place]
```

Подключите обработчик к `Http.Router` как POST на `Server.path`:

```flick
# import Http.ServerRequest exposing (ServerRequest)
# import Http.ServerResponse exposing (ServerResponse)
#
# route : (ServerRequest ->{} ServerResponse) -> Http.Router.Route none none
let route functions =
  Http.Router.route (Some Http.Method.POST) Server.path functions
```

## Что идёт по сети

Параметр (или кортеж из нескольких), результат и каждая ошибка передаются
как JSON, поэтому у каждого
есть свидетель Schema: запись или кортеж таких типов, `List`, `Option`,
тип с `deriving Schema` (и алиас типа тоже). Тип без свидетеля —
`FLICK-D001` на функции; остальную форму плагин проверяет при объявлении
функции (`std/E010`): стрелки до последней чистые,
каждый параметр назван, без переменных типа, ошибки перечислены.

## Публичный endpoint

Каждый `server fn` — публичный endpoint: его может вызвать с любым
параметром любой, у кого есть сетевой доступ к серверу, а не только страницы
программы. `flick doc` помечает каждую такую функцию.

## Кто вызывает

Функция, у которой среди ошибок есть `Server.Unauthorized`, защищена: её
реализация — `Server.Guarded`, и `Server.handler` принимает её только
через `Server.authenticated`. Тот на каждом вызове проверяет заголовки и
передаёт функции сервис, который сделала проверка; если проверка падает с
`Server.Unauthorized`, вызов получает эту ошибку, а тело не выполняется.
Защищённая функция без `Server.authenticated` или `Server.authenticated`
над функцией без `Server.Unauthorized` в ошибках — это `FLICK-E011`.

```flick
# import Http.ServerRequest exposing (ServerRequest)
# import Http.ServerResponse exposing (ServerResponse)
# import Server
#
service Session =
  user : Unit ->{} String

whoami : Unit ->{Server.Server, Session; Server.Unauthorized} String
server fn whoami () = Session.user ()

check : Http.Headers.Headers ->{; Server.Unauthorized} Session.Impl
let check headers =
  match Http.Headers.get "cookie" headers with
  | Some cookie -> { user = fun () -> cookie }
  | None -> Fail.fail Server.Unauthorized

# open_door : Unit ->{Server.Server} String
# server fn open_door () = "open"
#
# let example = task {
#   scope s {
# functions : (ServerRequest ->{Async} ServerResponse)^{s}
let functions =
  Server.handler (Server.authenticated Session check [server whoami] ++ [server open_door])
# ()
#   }
# }
```

Вызывающий обрабатывает `Server.Unauthorized` как любую объявленную
ошибку. У каждого вызова своя сессия, одновременные вызовы не видят чужих.
Вызов в памяти при рендере страницы несёт заголовки запроса страницы,
включая cookie. Передать защищённую функцию в `Server.handler` напрямую —
ошибка типов, как и функцию, которой нужен `Session`, но без
`Server.Unauthorized`, под `authenticated`: сессию тогда пришлось бы давать
тому, кто её обслуживает.

## На Go-target

Серверный вход, собранный с `[build] target = "go"` (или `--target go`), обслуживает `server fn` так
же, как собранный для js-target: `Server.handler` отвечает на те же
POST-запросы к `Server.path`, вызовы в его процессе идут в памяти, а
остальные его вызовы уходят на origin `Server.with_origin`. Поэтому Go-бэкенд
обслуживает фронтенд, собранный для js-target из тех же модулей:
соберите клиентский вход для js, а серверный — для go, каждый со своим
`flick.toml`.

Ни один из серверов не добавляет CORS-заголовков: страница с того же
origin, что и функции, вызывает их как есть, а фронтенду с другого origin
нужен CORS-middleware на роутере `Server.handler` — на любом target.
