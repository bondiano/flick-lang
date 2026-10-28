---
title: "Серверные функции"
---

`server fn` — функция, которая выполняется на сервере и которую любой код,
в том числе браузерный, вызывает по имени. Из одного объявления компилятор
строит endpoint, клиентский вызов и кодеки всего, что идёт по сети, поэтому
стороны не могут разойтись.

```flick
module App.Shop exposing (Order, OutOfStock, place)

type alias Order = { item : String; count : Number }
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
`String -> Number ->{Server.Server;} String` принимает два параметра. Вызывающий код видит другой тип:

```flick
place : Order ->{; Rpc.ClientError, OutOfStock} String
```

Вызову не нужны сервисы, поэтому клиентский вход может до него дойти
(`target = "client"`, см. `FLICK-E009`), и это
обычная задача: `Atom.task (task { place order })` или тело action.
Объявленная ошибка возвращается как есть; `Rpc.ClientError` значит, что
вызов не дошёл до сервера или ответ не декодировался. В браузере вызов идёт
на origin страницы, вне браузера — на `FLICK_SERVER_URL`, по пути
`Server.path` (`/_flick/fn`).

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
serve : forall s. Unit ->{Scope s, Server.Server, App.Db.Db;} (ServerRequest ->{} ServerResponse)^{s}
let serve () = Server.handler [server App.Shop.place]
```

Подключите обработчик к `Http.Router` как POST на `Server.path`:

```flick
let route functions =
  Http.Router.route (Some Http.Method.POST) Server.path (fun request -> functions request)
```

## Что идёт по сети

Параметр (или кортеж из нескольких), результат и каждая ошибка передаются
как JSON, поэтому у каждого
есть свидетель Schema: запись или кортеж таких типов, `List`, `Option`,
тип с `deriving Schema`. Компилятор проверяет это при объявлении функции
(`FLICK-E010`) вместе с остальной формой: стрелки до последней чистые,
каждый параметр назван, без переменных типа, ошибки перечислены.

## Публичный endpoint

Каждый `server fn` — публичный endpoint: его может вызвать с любым
параметром кто угодно, кто дотянется до сервера, а не только страницы
программы. Проверяйте, кто вызывает, внутри функции. `flick doc` помечает
каждую такую функцию.
