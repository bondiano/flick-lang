---
title: "Маршруты"
---

URL страницы — значение типа, который вы пишете. `deriving Url` даёт типу
принтер и парсер по его конструкторам, `Atom.route` держит текущую страницу
в атоме, а `Atom.loader` загружает её данные: на сервере — для
запрошенного URL, в браузере — после каждого перехода.

```flick
module App.Pages exposing (..)

type error NotFound = NotFound
  deriving Schema

type Page =
  | Home
  | User Float
  | Search { q : String; page : Option Float }
  deriving Eq, Show, Schema, Url

page : Atom.Writable Page
let page = Atom.route Page.url Home

profile : Atom.Atom (Atom.Async String {NotFound})
let profile =
  Atom.serializable_result
    (Codec.json Schema.string)
    (Codec.json (schema NotFound))
    (Atom.loader page (fun p -> task {
      match p with
      | User id -> App.Api.user_name id
      | _ -> ""
    }))
```

## URL типа

Тип с `deriving Url` выводит и `Schema`. Каждый конструктор — путь:

| Конструктор | URL |
|---|---|
| `Home`, `Index` | `/` |
| `Settings` | `/settings` |
| `User 42` | `/user/42` |
| `UserPost 7 slug` | `/user-post/7/<slug>` |
| `Search { q = "atoms"; page = None }` | `/search?q=atoms` |
| `Editor None`, `Editor (Some slug)` | `/editor`, `/editor/<slug>` |
| `Profile "ada" (Some Favorites)` | `/profile/ada/favorites` |

- Имя конструктора в kebab-case — первый сегмент.
- Каждое поле после него — сегмент: `String`, `Float`, `Bool` или
  уточнённый тип над одним из них либо тип, конструкторы которого без полей:
  он записывается именем конструктора в kebab-case (`Favorites` —
  `favorites`). Такой тип даёт слова вложенного пути: `type Tab = Favorites` и
  `Profile String (Option Tab)` выше.
- Последний сегмент может быть `Option` одного из них: `None` убирает его из
  URL, поэтому у одного конструктора есть URL и с ним, и без него.
- Последним сегментом может быть и другой тип с `deriving Url`: его маршрут —
  остаток пути. `Index` не добавляет сегмента, а слово выигрывает у
  параметра, поэтому при `type ArticleRoute = Feed | Index Slug` и
  `Articles (Option ArticleRoute)` путь `/articles/feed` — это
  `Articles (Some Feed)`, а `/articles/hi` — `Articles (Some (Index hi))`.
  Так один тип называет все пути REST API
  (`tests/run/schema/route_nested.fl`).
- Последнее поле-запись — query. Его поля тех же типов или `Option` одного
  из них; такого параметра в URL может не быть.
- Другое поле, обобщённый тип или два конструктора с одинаковыми URL —
  `FLICK-D009` с путём поля.

`Route.print Page.url value` даёт путь и query, закодированные как UTF-8.
`Route.parse Page.url text` даёт `Some value` или `None`, если ни один
конструктор не подошёл или поле не проходит свой тип, включая проверки
уточнённых типов. Origin и фрагмент он пропускает. Печать и разбор
обратимы для всякого значения, чей текст — корректный Unicode:
`tests/run/schema/route.fl` проверяет это `Arbitrary.check` над
`schema Page`.

## Маршруты как пути API

Там, где ожидается `Symbol`, конструктор типа маршрутов означает свой путь,
а на месте каждого параметра стоит его тип: при
`Profiles Username (Option Follow)` запись `Profiles Username (Some Follow)` —
это `"/profiles/:username/follow"`. Параметр называется по своему типу в
snake_case, поэтому у него должен быть собственный тип, например
уточнённый; поле `Option` записывается как `None` или `Some …`, поле слов или
вложенного типа маршрутов — одним из его конструкторов. Тип маршрутов
объявлен в другом модуле. Так эндпоинты `deriving HttpApi` называют свои
пути (см. [HttpApi](/ru/reference/http-api/#api-как-тип)); шаблон, который не
является путём, — `FLICK-D009`.

## Текущая страница

`Atom.route url fallback` — состояние со страницей URL браузера, или
`fallback`, если ни один конструктор не подошёл. Запись в него — переход:
URL нового значения добавляется в историю, а кнопка «назад» возвращает
прежнее значение.

У сервера нет адреса браузера. Обработчик кладёт запрошенный URL в реестр
запроса через `Atom.serving` до рендера, так что разметка и корень,
который её гидрирует, совпадают:

```flick
page_html : String ->{} List String
let page_html url =
  provide Atom.layer {
    Atom.serving url
    Stream.run_collect (Ui.Server.stream "app" (Ui.render App.Pages.view ()))
  }
```

## Данные страницы

`Atom.loader route load` — task-атом `load` от значения маршрута. Он
запускается заново при каждой смене маршрута. Если сделать его
сериализуемым, клиент начинает с результата, который сервер загрузил для
запрошенного URL, и не загружает его ещё раз.

`Atom.serializable_result wire error_wire atom` — это `serializable_task`,
который передаёт и неудачу с одной типизированной ошибкой, закодированной
`error_wire`: клиент показывает `NotFound` сервера, не запрашивая снова.
Дефект или прерывание не передаются никогда, так что стек и секреты не
покидают сервер; для них клиент запускает задачу сам.
