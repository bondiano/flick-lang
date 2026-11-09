---
title: "Routes"
---

A page's URL is a value of a type you write. `deriving Url` gives the type
a printer and a parser from its constructors, `Atom.route` keeps the
current one in an atom, and `Atom.loader` loads its data, on the server for
the requested URL and in the browser after each navigation.

```flick
module App.Pages exposing (..)
# import App.Api

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
    (Codec.json NotFound.schema)
    (Atom.loader page (fun p -> task {
      match p with
      | User id -> App.Api.user_name id
      | _ -> ""
    }))
```

## URLs of a type

A type that derives `Url` derives `Schema` too. Each constructor is a path:

| Constructor | URL |
|---|---|
| `Home`, `Index` | `/` |
| `Settings` | `/settings` |
| `User 42` | `/user/42` |
| `UserPost 7 slug` | `/user-post/7/<slug>` |
| `Search { q = "atoms"; page = None }` | `/search?q=atoms` |
| `Editor None`, `Editor (Some slug)` | `/editor`, `/editor/<slug>` |
| `Profile "ada" (Some Favorites)` | `/profile/ada/favorites` |

- The constructor's name in kebab case is the first segment.
- Each field after it is a segment: a `String`, a `Float`, a `Bool` or a
  refined type over one of them, or a type whose constructors have no
  fields, written as the constructor's name in kebab case (`Favorites` is
  `favorites`), or an enum, written as its literal (`Get = "GET"` is
  `GET`), which must hold no `/`, `?` or `#`. Such a type gives the words
  of a nested path:
  `type Tab = Favorites` and `Profile String (Option Tab)` above.
- The last segment may be an `Option` of one: `None` leaves it out of the
  URL, so one constructor has the URLs with and without it.
- The last segment may also be another type that derives `Url`: its route
  is the rest of the path. `Index` adds no segment, and a word wins over a
  parameter, so with `type ArticleRoute = Feed | Index Slug` and
  `Articles (Option ArticleRoute)`, `/articles/feed` is
  `Articles (Some Feed)` and `/articles/hi` is
  `Articles (Some (Index hi))`. One type names every path of a REST API
  this way (`tests/run/schema/route_nested.fl`).
- A final record field is the query. Its fields have the same types, or an
  `Option` of one, which may be missing from the URL.
- Any other field, a generic type or two constructors with the same URLs are
  `std/D009`, which names the field.

The plugin of `std` writes `Page.url` with `Route.codec Page.schema
[Route.path Home "" [] None false, …]`, a segment `Route.segment`,
`Route.words` or `Route.nested`, a query `Route.query`.

`Route.print Page.url value` gives the path and query, percent-encoded as
UTF-8. `Route.parse Page.url text` gives `Some value`, or `None` when no
constructor matches or a field fails its type, refined checks included. It
ignores the origin and the fragment. Printing and parsing round-trip for
every value whose text is well-formed Unicode: `tests/run/schema/route.fl`
checks it with `Arbitrary.check` over `Page.schema`.

## Routes as the paths of an API

Where a `Symbol` is expected, a constructor of a route type stands for its
path, with the type of each parameter in place of a value: given
`Profiles Username (Option Follow)`, `Profiles Username (Some Follow)` is
`"/profiles/:username/follow"`. A parameter is named after its type in
snake case, so it is a type of its own, such as a refined type; an `Option`
field is written `None` or `Some …`, a field of words or of a nested route
type as one of its constructors. The route type is declared in another
module. This is how the endpoints of `deriving HttpApi` name their paths
(see [HttpApi](../http-api/#an-api-as-a-type)); a template that is
no path is `FLICK-D009`.

## The current page

`Atom.route url fallback` is a state holding the page of the browser's URL,
or `fallback` when no constructor matches. Writing it navigates: the new
value's URL is pushed to the history, and the back button gives the earlier
value back.

A server has no browser location. A handler sets the requested URL in the
request's registry with `Atom.serving` before it renders, so the markup and
the root that hydrates it agree:

```flick
# import App.Pages
#
page_html : String ->{Async;} List String
let page_html url =
  provide Atom.layer {
    Atom.serving url
    Stream.run_collect (Ui.Server.stream "app" (Ui.render App.Pages.view ()))
  }
```

## Loading a page's data

`Atom.loader route load` is a task atom of `load` of the route's value. It
runs again whenever the route changes. Made serializable, the result the
server loaded for the requested URL is where the client starts, so the
client does not load it again.

`Atom.serializable_result wire error_wire atom` is `serializable_task` that
also hands over a failure of one typed error, coded by `error_wire`: the
client shows the server's `NotFound` without asking again. A defect or an
interruption is never handed over, so no stack or secret leaves the server;
for those the client runs the task itself.
