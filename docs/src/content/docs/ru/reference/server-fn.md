---
title: "Серверные функции"
---

`server fn` — функция, которая выполняется на сервере и которую любой код,
в том числе браузерный, вызывает по имени. Это блок плагина `server` пакета
`std`, который файл импортирует из `Server`; из одного объявления плагин
пишет endpoint, клиентский вызов и кодеки всего, что идёт по сети, поэтому
стороны не могут разойтись.

```flick
module App.Shop exposing (Order, OutOfStock, place)

import Server exposing (plugin server)
# import App.Db

type alias Order = { item : String; count : Float }
  deriving Schema

type error OutOfStock = OutOfStock String
  deriving Schema

/// Не больше трёх штук.
place : Order ->{Server.Server, App.Db.Db; OutOfStock} String
server fn place order =
  if order.count > 3 then from_result (Err (OutOfStock order.item))
  else App.Db.save order
```

Сигнатура прямо над `server fn` описывает то, что выполняет сервер:
параметры, нужные сервисы и ошибки. Выполняется только последняя стрелка:
`String -> Float ->{Server.Server;} String` принимает два параметра. Вызывающий код видит другой тип:

```flick
# type alias Order = { item : String; count : Float }
# type error OutOfStock = OutOfStock String
#
place : Order ->{; Rpc.ClientError, OutOfStock} String
# let place order = order.item
```

Вызову не нужны сервисы, поэтому клиентский вход может до него дойти
(`target = "client"`, см. `FLICK-E009`), и это
обычная задача: `Atom.task (task { place order })` или тело action.
Объявленная ошибка возвращается как есть; `Rpc.ClientError` значит, что
вызов не дошёл до сервера или ответ не декодировался. В браузере вызов идёт
на origin страницы, вне браузера — на `FLICK_SERVER_URL`, по пути
`Server.path` (`/_flick/fn`).

Секреты остаются на сервере так же: `Config.redacted` требует
`Server.Server`, поэтому клиентский вход, который прочёл бы пароль или ключ,
получает `FLICK-E009`, и ни имени секрета, ни его значения нет в бандле
страницы.

Пока `Server.handler` обслуживает функцию в процессе, её вызов в том же
процессе (так делает страница, отрендеренная на сервере) выполняется в
памяти: с сервисами обработчика, заголовками обслуживаемого запроса и
кодеками для всего, что передаётся, как по HTTP. Задача атома, запущенная
при рендере страницы, тоже попадает в трейс запроса.

## Обслуживание

`server place` — реализация, `Server.Function r`, где строка `r` содержит
`Server` и всё, что нужно телу. `Server.handler` обслуживает список таких
функций на одном маршруте; клиентский вход, назвавший `server place`,
доходит до `Server` и отклоняется.

```flick
# import Http.ServerRequest exposing (ServerRequest)
# import Http.ServerResponse exposing (ServerResponse)
# import Server exposing (plugin server)
# import App.Db
# import App.Shop
#
serve : forall s. Unit ->{Scope s, Server.Server, App.Db.Db;} (ServerRequest ->{} ServerResponse)^{s}
let serve () = Server.handler [server App.Shop.place]
```

Подключите обработчик к `Http.Router` как POST на `Server.path`:

```flick
# import Http.ServerRequest exposing (ServerRequest)
# import Http.ServerResponse exposing (ServerResponse)
#
# route : (ServerRequest ->{} ServerResponse) -> Http.Router.Route none none
let route functions =
  Http.Router.route (Some Http.Method.POST) Server.path (fun request -> functions request)
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
параметром кто угодно, кто дотянется до сервера, а не только страницы
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
# import Server exposing (plugin server)
#
service Session =
  user : Unit ->{} String

whoami : Unit ->{Server.Server, Session; Server.Unauthorized} String
server fn whoami () = Session.user ()

check : Http.Headers.Headers ->{; Server.Unauthorized} Session.Impl
let check headers =
  match Http.Headers.get "cookie" headers with
  | Some cookie -> { user = fun () -> cookie }
  | None -> from_result (Err Server.Unauthorized)

# open_door : Unit ->{Server.Server;} String
# server fn open_door () = "open"
#
# let example = task {
#   scope s {
# functions : (ServerRequest ->{} ServerResponse)^{s}
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
