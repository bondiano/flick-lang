---
title: "User interfaces"
---

`Ui` (see [Type-level programming and eDSLs](../../spec/08-type-programming-and-edsl/)) describes a user interface and the handlers of
its events. A handler's task writes atoms (`Atom.Write`) and nothing else:
it provides any other service itself and handles its own errors, so a
`Ui` carries no rows. Building one is pure: it runs no
handler, reads no state and touches no DOM. `Ui.Dom.mount` renders it with
React 19 (`react-dom/client`) and runs each event as a task.

```flick
type alias SaveButtonProps c = {
  label : String;
  disabled : Bool;
  on_save : (Unit ->{Atom.Write;} Unit)^c;
}

save_button : SaveButtonProps c -> Ui^c
let save_button props = ui {
  button
    {
      disabled = props.disabled;
      on_click = Ui.Event.exhaust (fun _ -> task { props.on_save () });
    }
    [text props.label]
}
```

`c` is a capture row (see [regions](../regions/#captures)). Like any new type
variable of a signature it is quantified over the whole signature, so
`forall c.` can be left out ([types](../types/)).

The two `^c` are what let `save_button` accept any callback. A function type
without a qualifier holds no regions. So with `on_save : Unit ->{Atom.Write;} Unit`, a callback that uses a `File s` from an enclosing `scope`
is rejected where it is passed (`FLICK-G001`), even when the `Ui` never
leaves the scope. A callback that needs a service, such as saving through
`Users`, provides it and handles its errors inside, writing the outcome to
an atom. With `^c` on the field the callback may hold anything, and
`^c` on the result keeps those captures in the `Ui`, so a `Ui` that holds a
region still cannot leave its scope. A qualifier on the result alone does not
help, since the field would still allow no captures. A top-level signature is
checked as written, so these qualifiers are never inferred. A component whose
callbacks never hold scoped resources can drop both of them.

`ui { … }` is a block in which the names `Ui` exports need no `Ui.`: a local
binding still wins, then a name of `Ui`, then the module's own names.
Ordinary calls such as `ui { button … [text …] }` compile to the same IR
as `Ui.button … [Ui.text …]`. Applying a value of type `Ui.Component p`
to props inside the block also inserts `Ui.component` automatically.

## Elements and props

Props are exact records: a missing field, an unknown one or a wrong type is
an error. A key is metadata, given with `Ui.keyed`, never a prop.

| Function | Element | Children |
| - | - | - |
| `text s` | text, through React's text channel: markup stays text | none |
| `fragment`, `optional`, `empty` | no element | `List`, `Option`, none |
| `div`, `span` `{ class_name }` | `div`, `span` | `List Ui` |
| `paragraph`, `heading`, `list`, `item` | `p`, `h1`, `ul`, `li` | `List Ui` |
| `heading_at level` | `h1` … `h6` for `H1` … `H6` | `List Ui` |
| `button { disabled; on_click }` | `button type="button"` | `List Ui` |
| `input { value; placeholder; on_input }` | `input`; `value` is the initial text | none |
| `textarea { value; placeholder; on_input }` | `textarea`, as `input` over several lines | none |
| `pre { class_name }` | `pre`: spaces and line breaks kept | `List Ui` |
| `link { href : SafeUrl }` | `a` | `List Ui` |
| `route_link { route; url; to }` | `a` to the page `to`, navigating without a reload (below) | `List Ui` |
| `nav`, `footer`, `fieldset` `{ class_name }` | `nav`, `footer`, `fieldset` | `List Ui` |
| `icon { class_name }` | `i` of an icon font, which its classes name | none |
| `hr` | `hr` | none |
| `image { src : SafeUrl; alt; class_name }` | `img` | none |
| `form { on_submit }` | `form`: a submit runs `on_submit` and never reloads | `List Ui` |
| `submit { disabled }` | `button type="submit"`, which submits the form around it | `List Ui` |
| `html (s : SanitizedHtml)` | `div` with the HTML | none |
| `select (SelectProps a c)` | `ul role="listbox"`; choosing an item runs `on_change` with it | items through `render_item` |
| `panel { header; body; footer }` | `section` with `header`, `main`, `footer` | named places |
| `tabs { selected } (NonEmpty Tab)` | a tab list and the selected panel | `Tab`s from `tab label ui` |
| `c props` where `c : Ui.Component p` | a foreign React component | its props |

`text` is `String -> Ui`, so it sits among the children of
any element. `SafeUrl` comes only from `Ui.safe_url`, which accepts relative
URLs and the schemes `http`, `https` and `mailto` and refuses control
characters; `SanitizedHtml` only from `Ui.trusted_html`, the program's own
word. A `String` is neither. A foreign component is a `Ui.Component p` bound
with `foreign pure`; React renders it, Flick never calls it.

Inside `ui`, write `badge props` instead of `Ui.component badge props`:

```flick
badge : Ui.Component { label : String }
foreign pure badge from "./badge.mjs" "Badge"

example : Ui
let example = ui { div { class_name = "badges" } [badge { label = "new" }] }
```

This also works for local bindings, record fields, values returned by
functions and pipes (`props |> badge`). Only application is shorthand:
a bare `badge` keeps its `Ui.Component` type. Props and capture checks
are unchanged. Outside `ui`, use `Ui.component badge props`.

`with_host attrs node` puts host attributes on an element: `id`,
`class_name`, `role`, `aria_label`, `aria_describedby`, `aria_hidden`,
`aria_invalid`, `aria_pressed`, `aria_live` (`Polite`, `Assertive`, `Off`), `tab_index`,
`data` (pairs written `data-<name>`), the text-entry hints
`auto_capitalize`, `auto_correct` and `wrap`, and a form field's `name` and
`input_type` (`TextInput`, `PasswordInput`, `EmailInput`, its `type`). Each is
typed and optional:
`with_host { host with role = Some "status" } (span { class_name = "s" } […])`.
It gives every element its classes the same way, those whose props have no
`class_name` included: `with_host { host with class_name = Some "nav-link" } (route_link …)`.

`route_link { route; url; to } children` is a link inside the app to the page
`to` of a route state, `route = Atom.route url fallback`. Its `href` is
`Route.print url to`, so a server's markup, a click for a new tab and a
middle click follow it as the browser does. A plain left click, without a
modifier key, writes `to` to `route` instead: the URL is pushed to the history
without a reload, and the views that read `route` render the new page.

## Atoms and views

An atom (`Atom`) describes a value; the registry a program provides with
`provide Atom.layer { … }` holds the values of the atoms read
through it. `Atom.make value` is a state, `Atom.derived` an atom computed
from the atoms it reads, and `Atom.set`/`Atom.update` write a state
through the service `Atom.Write`, which the task of an action has and a
render does not. `Atom.layer` provides one registry as both
`Atom.Registry`, which roots and server markup read, and `Atom.Write`;
`Atom.Registry` is Effect's `Reactivity.Atom.Registry`, which `Atom`
re-exports.

A view is a component whose render reads atoms through the service
`Atom.Read`. `Ui.Dom.mount` renders a `Ui` with the provided registry, and
each view in it renders again only when an atom it read changes, not when a
sibling's does:

```flick
count : Atom.Writable Float
let count = Atom.make 0

counter_view : Ui.View Unit
let counter_view =
  Ui.view (fun () ->
    ui {
      button
        {
          disabled = false;
          on_click = Ui.Event.exhaust (fun _ -> task { Atom.update count (fun n -> n + 1) });
        }
        [text (Atom.value count |> show)]
    })
```

`Ui.render view props` places a view in a `Ui`. The compiler names each
top-level view, action and atom `Module.name`, as React's developer tools
and Effect's show it (`App.Counter.counter_view`); a release build keeps
them all, or only those of views and actions with `[build] labels = "components"`.
`Ui.named` gives a view another name. A view subscribes to what it read after React
commits, and a write between its render and its subscription renders it
again. Rendering a view outside `Ui.Dom.mount` fails with what to do. The
render reads synchronously: like a derived atom's computation, its only
service is `Atom.Read`. So it is a Sync stage: it compiles to a plain
function, and `Atom.get` in it is a direct call, without an Effect. A
write, another service or an error in it is
`FLICK-U004`: write atoms in an action, run a task as `Atom.task` and read
its result, or give failures a view with `Ui.suspended`.

An atom is made once and keeps its identity. A top-level atom has one
type: `let items = Atom.make []` would hold one list for every element
type, so its type needs no variables, and `FLICK-U005` asks for the type of
its value. A local atom without a type is not generalized, so its uses
decide its one type; a signature, an annotation or a higher-rank argument
that gives it variables is `FLICK-U005` too. Nothing is made while
rendering either: an atom, an action or a
view made in a render, which may run again at any time, would start over on
every render, so `FLICK-U006` refuses it, and a call of any function that
makes one, in the module or in a module it imports. Make each with a
top-level `let`, and one atom per key with `Atom.family`: `Atom.at family
key` gives the same atom for an equal key, which needs `Hash`, so it holds
no function. A family holds its atoms weakly, but the registry keeps a
state's value while it lives: `Atom.forget family key` lets the state of a
deleted item go once no view reads it, and the next `Atom.at` makes a new
one. The task of a handler runs on an event, not while rendering, and may
make what it needs.

```flick
drafts : Atom.Family Float (Atom.Writable String)
let drafts = Atom.family (fun _ -> Atom.make "")

remove : Action.Action (Float, Ui.ClickSnapshot)
let remove =
  Action.exhaust (fun (id, _) ->
    task {
      Atom.update items (List.filter (fun i -> i != id))
      Atom.forget drafts id
    })
```

`Ui.keyed id node` gives a dynamic sibling a stable identity. Keys are
local to a sibling list and must be unique there. Reordering preserves both
the React subtree and the event policy state of that key; it must not start
another task for a busy `exhaust` handler.

### Components

`component name (props : P) = body` declares a view of its props, of the
type `Ui.View P`, which `Ui.render name props` places. The props are one
pattern, usually a record with its type. The body is a block: its last
expression renders the component, again whenever an atom it read changes or
its props do. A body that is that expression alone is
`let name = Ui.view (fun (props : P) -> e)`:

```flick
component badge (props : { label : String }) = ui { text props.label }

component card (person : { name : String; label : String }) =
  ui { div { class_name = "card" } [text person.name, Ui.render badge { label = person.label }] }
```

The statements before the last expression are the setup of each instance.
It runs once, when the instance mounts, with the service `Setup s` of the
instance's region `s`, and its names are in scope in the last expression,
which renders with them. `Atom.local` makes an atom of the instance and the
`Action` makers make its actions there; a render that makes one is
`FLICK-U006`, and `Atom.local` outside a setup is an effect error. The setup
runs once, so it reads no atom: the render does, `ui { let n = … }` being a
block too, or an `Atom.derived` made in the setup does. The component is
`Ui.stateful (fun props -> setup; fun props -> render)`, of the type
`forall s. P ->{Setup s} (P ->{Atom.Read} Ui)^{s}`: the setup sees the
first props, the render the current ones. Hovering `component` shows that
type.

```flick
component counter (start : Float) =
  let count = Atom.local start
  let bump = Action.exhaust (fun _ -> task { Atom.update count (fun n -> n + 1) })
  ui {
    let n = Atom.value count
    button { on_click = bump; disabled = false } [text (show n)]
  }
```

The setup runs once per mounted instance, under React's StrictMode too.
When an instance unmounts, the events of the actions its setup made stop
and their tasks are interrupted; the root still waits for those tasks
when it closes.

#### Work of an instance

`Ui.on_mount task` in a setup runs `task` once the instance mounts, as a
task of the root: its unmount interrupts it and waits for it, its
finalizers run, and a defect goes to the root's supervisor.
`Ui.watch read run` runs `run` of what `read` computes, and again whenever
an atom `read` read changes, interrupting the previous run first. Both are
React's `useEffect` without a dependency list: `on_mount` runs once per
instance, `watch` tracks the atoms it read. Under StrictMode the replayed
mount stops the first run before the second starts, and a server renders
without starting either. Their tasks write atoms (`{Atom.Write}`), as an
action's do.

```flick
component clock () =
  let seconds = Atom.local 0
  let second = Result.with_default Duration.zero (Duration.seconds 1)
  let _ticking =
    Ui.on_mount
      (task {
        Effect.forever
          (task {
            Effect.sleep second
            Atom.update seconds (fun n -> n + 1)
          })
      })
  ui { text (show (Atom.value seconds)) }
```

#### Elements of an instance

`Ui.element ()` in a setup makes an element of the instance, of a tag
type: `Ui.Input`, `Ui.Textarea`, `Ui.Button` or `Ui.Div`. A render binds it
to a node with `input_at element props`, `textarea_at`, `button_at` or
`div_at`, so the type checks the tag; it only passes the element on. A task
of an action, `Ui.on_mount` or `Ui.watch` reaches the node through the
service `Ui.Document`, which a root provides: `Ui.focus`, `Ui.blur`,
`Ui.scroll_into_view` and `Ui.measure` (`{ width; height; top; left }`). An
element with no mounted node fails with the error `Ui.Detached`, not a
`null`: before its instance renders, after it unmounts, or under a
renderer without a DOM. A render, and a server, have no `Ui.Document`, so a
DOM operation there is a type error; a hydrated element binds to the node
the server rendered.

```flick
component search () =
  let field = Ui.element ()
  let focus = Action.exhaust (fun _ -> task { try Ui.focus field with | Ui.Detached -> () })
  ui {
    div
      { class_name = "search" }
      [
        input_at field { value = ""; placeholder = "find"; on_input = Ui.Event.latest (fun _ -> task { () }) },
        button { disabled = false; on_click = focus } [text "focus"],
      ]
  }
```

It is React's `useRef` for the DOM alone, typed by tag; Elm reaches a node
with `Browser.Dom.focus` and a string id instead, which no type checks.
A widget that drives the DOM imperatively stays a foreign component,
`Ui.component`.

What an instance owns holds its region: `Atom.local` gives
`(Atom.Writable a)^{s}` and `Ui.element` `(Ui.Element t)^{s}`, so neither
can be stored where something outside the instance reads it
(`FLICK-G001`), and the registry forgets an instance's atoms when it
unmounts. The atom functions borrow what they take, `(Atom.Writable a)^d`,
so they read and write such an atom. A function that makes the state of an
instance says so in its signature,
`Float ->{Setup s;} (Atom.Writable Float)^{s}`, and is called from a
setup; an annotation names the capture too, `(Ui.Element Ui.Input)^c`. The
word `component` stays a name everywhere else.

## Tasks in views

`Atom.task t` is an atom of the result of the task `t`, run by the registry
that first reads it: `Atom.Async a e` is `Loading`, `Loaded a` or
`Failed (Failure e)`. A view reads it like any atom and matches the three
cases; there is no Suspense by default, so loading and failure are visible
where the value is read.

`Ui.suspended failed fallback render` is the opt-in: its render may read a
task with `Atom.ready`, which gives the value of a loaded task. While one
loads the view shows `fallback`; when one failed, `failed` of the failure.
The errors `Atom.ready` raises are in the render's error row `x`, so a task
that can fail needs that view to have somewhere to put them. `Atom.ready`
in a plain `Ui.view` of a task still loading is a defect that says so.

`Ui.awaiting failed fallback render` is that view for one task, the atom
given as its props: a short screen needs no `match` and no `Atom.ready`.
Its failure view is still required; a task of `Async a none` never calls it.

```flick
user : Atom.Atom (Atom.Async String {Offline})
let user = Atom.task (task { fetch_name () })

// matching the three cases by hand
user_view : Ui.View Unit
let user_view =
  Ui.view (fun () ->
    match Atom.get user with
    | Atom.Loading -> ui { text "…" }
    | Atom.Loaded name -> ui { text name }
    | Atom.Failed _ -> ui { text "offline" })

// the same with `Ui.awaiting`, rendered as `Ui.render awaited user`
awaited : Ui.View (Atom.Atom (Atom.Async String {Offline}))
let awaited = Ui.awaiting (fun _ -> ui { text "offline" }) (ui { text "…" }) (fun name -> ui { text name })
```

### Queries and mutations

`Atom.query keys t` is `Atom.task t` that a mutation of one of `keys` runs
anew. `Atom.mutation keys f` is `f` that, when it succeeds, runs anew every
query of those keys its registry reads; one that fails changes nothing.
Queries of other keys keep their value. `Atom.query_stream keys s` shows
the latest element of the stream `s` and starts it anew on such a
mutation; it has no mutation of its own. Over a [server function](../server-fn/):

```flick
orders : Atom.Atom (Atom.Async (List Order) {Rpc.ClientError})
let orders = Atom.query ["orders"] (task { Shop.orders () })

place : Order ->{Atom.Write; Rpc.ClientError, OutOfStock} String
let place = Atom.mutation ["orders"] Shop.place
```

A query is a task atom: `Atom.serializable_task` hands its loaded value
from a server render to the client under the binding's name.

## Fields bound to a state

`Ui.field { bind = query; placeholder = "Find"; on_input = None }` is an
input that shows the state `query` and writes each keystroke to it at once,
inside the browser's event. React then keeps the caret where the user left
it and an IME composition intact, and a write to `query` from elsewhere, a
reset, shows in the field. `on_input`, when given, runs as a task with the
text after the state has it. `Ui.field_area` is the text area. Like views,
fields need the registry of `Ui.Dom.mount`.

## Actions

An action is a handler declared once, usually at the top level, and given to
as many elements as need it: `on_click = increment`. `Action.exhaust`,
`latest`, `queue` and `concurrent` take the policies of `Ui.Event`, but the
elements that use one action share its policy state in a root, so
`Action.exhaust` ignores a click on any of its buttons while the task of an
earlier one runs. The tasks belong to the root, which interrupts and joins
them when it unmounts.

```flick
increment : Action.Action Ui.ClickSnapshot
let increment = Action.exhaust (fun _ -> task { Atom.update count (fun n -> n + 1) })
```

`Action.with action arg` gives an action its argument as data: each element
of a list passes its item, `on_click = Action.with remove item`, and each
element has a policy state of its own, kept when a keyed list reorders: an
`exhaust` save of one item does not ignore the save of another. A handler made from a `fun` inside a
view's render is a new closure on every render; it works, but
`FLICK-U002` warns about it.

## Handlers and policies

A handler field has the opaque type `Ui.Handler snapshot`, which only a
policy of `Ui.Event` makes, so no handler is fire-and-forget:

| Policy | An event that comes while earlier tasks run |
| - | - |
| `exhaust f` | is ignored |
| `latest f` | interrupts the running task, waits until it stopped, then runs; one event waits at most, the newest |
| `queue capacity f` | waits, `capacity` events at most; beyond that it is rejected |
| `concurrent limit f` | runs, `limit` tasks at most; beyond that it is rejected |

`capacity` and `limit` are positive integers: a constant that is not one is
`FLICK-U001`, a computed one is checked when the handler is made. A
rejected event is an infrastructure defect for the supervisor, never an
error of `e`. `f` gets an inert snapshot taken inside the DOM callback —
`ClickSnapshot` (`button`, `alt`, `ctrl`, `shift`, `meta`), `InputSnapshot`
(`value`), or the item of a `select`. `Ui.Event.prevent_default decide h`
calls the pure `decide` on the snapshot before the callback returns and
prevents the default action when it says so; otherwise the default action
happens.

Handlers keep their captures: a button whose handler holds a file of a
scope cannot leave that scope (`FLICK-G001`). Building a `Ui` is pure, so
`Ref.get` there is a runtime call where none is allowed (`FLICK-E001`); a
view reads atoms instead.

An event prop keeps the same listener across committed renders, but uses
the current handler. Removing the prop or unmounting its element detaches
that listener and drops events still waiting in its policy queue. Tasks
already started remain root-owned: they may finish, and closing the root
still interrupts and joins them.

## Running a root

```flick
mount :
  forall c r.
  Ui.Dom.Container -> (Failure none ->{r; none} Unit)^c -> Ui^c ->{Atom.Registry, Atom.Write | r; none} Never
```

`Ui.Dom.mount container supervisor ui` renders `ui` once; the views in it
render again when the atoms they read change. The initial DOM mount is
synchronous, later renders are React 19's to schedule and coalesce. Each
task writes atoms with the `Atom.Write` of the `mount` call, the
supervisor gets its defects, and the registry is the one the program
provides:

```flick
main : Unit ->{; AppError} Unit
let main () =
  match Ui.Dom.container "app" with
  | None -> App.fail "missing root"
  | Some root ->
    provide Atom.layer {
      ignore (Ui.Dom.mount root report (Ui.render counter_view ()))
    }
```

A failure of a task, and each rejected event, goes to `supervisor`; an
interruption by a policy or by closing does not. A failure of the
supervisor itself ends `mount` as a defect.

`mount` lasts until the fiber running it is interrupted, usually by the end
of the scope that spawned it. Closing has two phases: React cleanup stops
taking events at once and unmounts the tree, removing its listeners; then
the handler tasks are interrupted and joined, supervisors included, and
only then `mount` ends — so the scope releases its resources after the
tasks have stopped. `Ui.Dom.container id` finds the element to render into.

The adapter is `@flick-lang/runtime/ui`; `react` and `react-dom` 19 are
peer dependencies of the runtime, installed by the application.

DOM roots live in the separate `Ui.Dom` module. `Ui` itself imports only
the React-only adapter, so compiled `Ui.mount_with` programs do not require
`react-dom`. DOM callers use `Ui.Dom.mount` and `Ui.Dom.container`; there
are no aliases in `Ui`.

## Server-rendered initial views

`Ui.Server.render ui` gives the markup of `ui`, its views reading atoms from
the provided registry. Give each request a registry of its own: requests
rendered at once then never see each other's atoms, and since the registry
is a service of the `provide`, nothing can render with it after the
`provide` ends (`FLICK-E002`). Handlers do not run on the server.

```flick
request : Float ->{} String
let request n =
  provide Atom.layer {
    Atom.set count n
    Ui.Server.render (Ui.render counter_view ())
  }
```

`Ui.Server` comes from `@flick-lang/runtime/ui/server`, so client programs
do not load `react-dom/server`. A JavaScript host can instead
expose an initial-view function from Flick and evaluate its cold effect on
the server. Pass the resulting `Ui` to `uiRenderStatic` from
`@flick-lang/runtime/ui/react`, then serialize that React node with
`renderToString` from `react-dom/server`. Its views and fields read atoms
as a server does, each call from a registry of its own. Event bindings
remain inert on the server; the application does not need a second HTML
implementation.

Start a server that renders views with `NODE_ENV=production`, as in
`NODE_ENV=production node dist/flick-server.js`. Without it React runs its
development build, which renders about twice as slowly.

Mark its container with `data-flick-ssr`. `Ui.Dom.mount` then uses
`hydrateRoot` instead of clearing the existing HTML. Server and client
must use the same initial state, keys and markup. Native textarea drafts,
focus and selection survive startup; adopt a pre-boot draft into the
Flick state before checking or submitting it. Mount the root before
starting background tasks that notify its event handlers.

The documentation header and Play/Transpile use this path. Without the
marker, ordinary client-only mounting is unchanged.

### Serializable atoms

`Atom.serializable wire state` marks a state whose value a server can hand
to the client, coded by the JSON codec `wire`. Its key is the name of the
top-level binding it defines, `Module.name`, which the compiler supplies; an
atom of `Atom.serializable_family` has the key `Module.name/key`. Anywhere
else, such as inside a function, applied without its state, or twice in one
binding, one key could name many atoms, and `FLICK-U003` refuses it. A
`PureCodec` codes synchronously and needs no services.

```flick
count : Atom.Writable Float
let count = Atom.serializable (Codec.json Schema.number) (Atom.make 0)

draft : String -> Atom.Writable String
let draft = Atom.serializable_family (Codec.json Schema.string) (fun _ -> Atom.make "")
```

The server hands them over with `Ui.Server.state "app"`: a
`<script type="application/json" id="flick-state-app">` with the
registry's serializable atoms for the container `app`, to put in the page
once the container is rendered. `Ui.Dom.mount` on that container writes
them into its registry before its first render and removes the script, so
the client starts from the server's values rather than asking for them
again, and mounting it again does not write them over values changed
since. Each root of a page reads only the script of its own container. An atom whose value its codec does not
encode is a defect naming its key, not a value silently left out.

```flick
page : Float ->{} String
let page n =
  provide Atom.layer {
    Atom.set count n
    let markup = Ui.Server.render (Ui.render counter_view ())
    "<div id=\"app\" data-flick-ssr>" ++ markup ++ "</div>" ++ Ui.Server.state "app"
  }
```

### Server and client parity

Server markup hydrates only if the client's first render draws the same
thing. A view therefore reads each atom as the server would, both on the
server and while its root hydrates, and renders again with the client's
values once React has hydrated it.

A browser state such as the URL has no value on a server. Its type,
`Atom.Browser a`, is not an atom that `Atom.get` or `Atom.value` reads: give
it the value the server renders with `Atom.on_server`, or read it with
`Atom.client_only`, which is `None` on the server and while hydrating.
`Atom.search_param name` is the URL's search parameter `name`; a write
updates the URL.

```flick
query : Atom.Writable String
let query = Atom.on_server "" (Atom.search_param "q")

tab : Atom.Atom (Option String)
let tab = Atom.client_only (Atom.search_param "tab")
```

### Streaming

`Ui.Server.stream "app" ui` renders the container `<div id="app"
data-flick-ssr>` itself, in chunks as React renders them, for a response to
stream. A view of `Ui.suspended` that reads a loading task is sent as its
fallback, and its markup follows once the task ends; a task read outside
`Ui.suspended` is sent as `Loading`, and the stream does not wait for it.
Serializable atoms come in JSON scripts in the stream: the shell's before
it, and a serializable task's value before the markup that shows it.
`Atom.serializable_task wire task` marks a task atom whose loaded value the
server hands over this way, keyed like `Atom.serializable`; the client
starts from that value instead of running the task again. `Ui.Dom.mount` on
the container hydrates the scripts that arrive after it mounted, as they
arrive.

```flick
user : Atom.Atom (Atom.Async String none)
let user = Atom.serializable_task (Codec.json Schema.string) (Atom.task (task { fetch_name () }))

chunks : Unit ->{} List String
let chunks () = provide Atom.layer { Stream.run_collect (Ui.Server.stream "app" page) }
```

### Templates without React

`Ui.Server.to_html ui` gives the HTML of `ui` without React, for mail and
other markup no client hydrates. It is a pure function: text and attribute
values are escaped as `text` keeps markup in text, handlers are left out,
and views read atoms as a server does, from a registry of that call alone,
so the same `ui` always gives the same HTML. A foreign component, which
only React renders, is a defect. It comes from
`@flick-lang/runtime/ui/html`, which imports neither React nor
`react-dom/server`.

```flick
receipt : Order -> String
let receipt order =
  Ui.Server.to_html (ui { div { class_name = "mail" } [paragraph [text ("Thanks, " ++ order.customer)]] })
```

## Other React renderers

`@flick-lang/runtime/ui` is the React DOM adapter.
`@flick-lang/runtime/ui/react` contains the React-only node conversion,
views and task lifecycle; importing it does not load `react-dom` or
read `document` or `window`.

The latter exports `uiMountWith` and the renderer factory contract:

```typescript
type Renderer = () => {
  render(node: React.ReactNode): void;
  unmount(): void;
};
```

The factory is called when the root's Effect starts, not when its
description is built. Its `render` submits the supplied React node to the
chosen renderer; `unmount` synchronously detaches the root. The shared
runner stops taking events, unmounts, interrupts and joins task fibers
before returning. A renderer must unmount all its React components, not
just hide its surface.

Flick binds a factory as an opaque `Ui.Renderer` through `foreign pure`
and runs it with `Ui.mount_with renderer supervisor ui`. This uses the
same registry, services, errors, captures and shutdown contract as
`Ui.Dom.mount`.
It is the extension point for React Native, Ink and React Three Fiber,
not an implementation of those adapters. Existing `div`, `input` and
other HTML helpers remain DOM-specific; non-DOM surfaces need their own
typed primitives and event snapshots, or foreign React components through
`Ui.component`. Do not import the DOM entry point in a non-DOM adapter.

## Atoms, not a TEA program

The state of a Flick UI is in atoms: a view renders from the atoms it reads,
and a handler is a supervised task that writes them. There is no
`Msg → update` loop, and none is planned: what Elm puts in commands is a
task of an action, run by the Effect fibers, scopes and event policies that
already exist. Sources that write atoms on their own, Elm's subscriptions,
are among the [limitations](#limitations) below.

### Comparison with Elmish

The comparison is with [`elmish/react` v6.x](https://github.com/elmish/react/tree/v6.x),
not with a complete MVU framework. Its
[`common.fs`](https://github.com/elmish/react/blob/v6.x/react/common.fs)
provides `lazyViewWith` backed by `React.memo` and sibling keys (a Flick
view renders again only for the atoms it read, so it needs no comparator);
[`react.fs`](https://github.com/elmish/react/blob/v6.x/react/react.fs)
adds root-model identity checks, `requestAnimationFrame` batching,
hydration and `valueOrDefault`. The dispatch loop and subscriptions belong
to the separate [`elmish/elmish`](https://github.com/elmish/elmish) core.
Flick's React 19 batching does not promise Elmish's animation-frame schedule.

### Completed optimizations and evidence

- A view renders again only when an atom it read changes. React 19
  coalesces pending renders instead of forcing a synchronous commit after
  every task; only the initial DOM mount stays synchronous.
- Keys preserve subtree identity and busy event-policy state on reorder.
  Component-owned stable listeners receive the current handler after
  commit, without retaining an obsolete view.
- `Ui.Renderer`/`Ui.mount_with` share the React lifecycle without importing
  `react-dom`. `Ui.Dom` owns DOM mounting; tasks belong to the root, not
  to the DOM surface.

The implementation is in the
[React-only runner](https://github.com/bondiano/flick-lang/blob/main/runtime/src/ui/react/index.ts),
[DOM adapter](https://github.com/bondiano/flick-lang/blob/main/runtime/src/ui/index.ts)
and built-in modules
[`Ui`](https://github.com/bondiano/flick-lang/blob/main/flick-core/src/types/builtins/Ui.fl)
and [`Ui.Dom`](https://github.com/bondiano/flick-lang/blob/main/flick-core/src/types/builtins/Ui/Dom.fl).
Regression coverage is kept with the behavior:

- [`runtime/test/ui-roots.test.ts`](https://github.com/bondiano/flick-lang/blob/main/runtime/test/ui-roots.test.ts)
  and [`ui-policies.test.ts`](https://github.com/bondiano/flick-lang/blob/main/runtime/test/ui-policies.test.ts):
  views, keys, stable listeners, StrictMode, supervision, unmount/join and
  importing the React-only entry point without a DOM.
- View check fixtures
  ([pass](https://github.com/bondiano/flick-lang/blob/main/tests/check/pass/ui/views.fl),
  [fail](https://github.com/bondiano/flick-lang/blob/main/tests/check/fail/ui/children.fl)):
  captures, services and the supervisor contract.
- [`tests/run/ui/counter.fl`](https://github.com/bondiano/flick-lang/blob/main/tests/run/ui/counter.fl):
  atoms, actions and a custom renderer.
- [`tests/run/ffi/foreign.fl`](https://github.com/bondiano/flick-lang/blob/main/tests/run/ffi/foreign.fl):
  unary callback identity across FFI. The
  [compiler's foreign-call lowering](https://github.com/bondiano/flick-lang/blob/main/flick-core/src/codegen/js/items.rs)
  passes unary callbacks unchanged, avoiding a wrapper allocation;
  callbacks with several arguments still adapt the curried ABI.
- `compiled_ui_core_imports_without_a_dom_renderer` in
  [`flick/tests/e2e.rs`](https://github.com/bondiano/flick-lang/blob/main/flick/tests/e2e.rs):
  imports a compiled Flick module while rejecting `react-dom` imports,
  rather than checking only the runtime's TypeScript entry point.

### Limitations

These capabilities are **not implemented**. Supporting React Native, Ink or
React Three Fiber needs more than moving DOM tags: each needs the host
primitives and an adapter below.

| Capability | Minimum contract and reason |
| - | - |
| Keyed subscriptions of atoms | Start and stop sources by stable identity, reject late events and use scope-owned cleanup. Timers, sockets, terminal input and animation sources write atoms without waiting for a UI handler to finish. |
| Typed host primitives and events | Separate HTML constructors from renderer-specific `View`/`Text`, Ink host nodes and R3F scene nodes. Styles, numeric props and event snapshots must not assume DOM `target.value` or `preventDefault`. |
| Real Native / Ink / R3F adapters | Build on `Renderer` and host primitives: registration/mounting, updates, synchronous detachment and resource shutdown. Define R3F's frame-loop/invalidation relationship with atoms explicitly; do not send 60 messages per second unnecessarily. |
| Typed React interop | Foreign-component children/slots, scoped refs, Context/providers and lifecycle with defined resource release. Currently only opaque `Ui.component` with props is available. |

Existing Effect, fibers/scopes, bounded event
policies, supervision, escaping checks and the safe text channel are not
missing capabilities and need no duplicate APIs.
